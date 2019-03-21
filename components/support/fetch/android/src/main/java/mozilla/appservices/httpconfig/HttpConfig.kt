/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.httpconfig

import mozilla.appservices.support.RustBuffer
import mozilla.components.concept.fetch.Client
import mozilla.components.concept.fetch.MutableHeaders
import mozilla.components.concept.fetch.Request
import mozilla.components.concept.fetch.Response
import java.lang.Exception
import java.lang.RuntimeException
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

object RustHttpConfig {
    @Volatile
    private var imp: CallbackImpl? = null
    private var client: AtomicReference<Client?> = AtomicReference(null)

    @Synchronized
    fun setClient(c: Client) {
        if (!client.compareAndSet(null, c)) {
            throw RuntimeException("Already initialized Rust HTTP config!")
        }
        if (imp != null) {
            // This should never happen, but if it *did* happen, it's memory unsafe
            // for us to ever clear it, so we check anyway.
            throw RuntimeException("Imp set without client?")
        }
        imp = CallbackImpl()
        LibSupportFetchFFI.INSTANCE.support_fetch_initialize(imp!!)
    }

    internal fun doFetch(b: RustBuffer.ByValue): RustBuffer.ByValue {
        try {
            val request = MsgTypes.Request.parseFrom(b.asCodedInputStream())
            val headers = MutableHeaders()
            for (h in request.headersMap) {
                headers.append(h.key, h.value)
            }
            val conceptReq = Request(
                    url = request.url,
                    method = convertMethod(request.method),
                    headers = headers,
                    connectTimeout = Pair(request.connectTimeoutSecs.toLong(), TimeUnit.SECONDS),
                    readTimeout = Pair(request.readTimeoutSecs.toLong(), TimeUnit.SECONDS),
                    body = if (request.hasBody()) {
                        Request.Body.fromString(request.body )
                    } else {
                        null
                    },
                    redirect = if (request.followRedirects) {
                        Request.Redirect.FOLLOW
                    } else {
                        Request.Redirect.MANUAL
                    },
                    cookiePolicy = if (request.includeCookies) {
                        Request.CookiePolicy.INCLUDE
                    } else {
                        Request.CookiePolicy.OMIT
                    },
                    useCaches = request.useCaches
            )
            val rb = try {
                val resp = client.get()!!.fetch(conceptReq)
                val rb = MsgTypes.Response.newBuilder()
                        .setUrl(resp.url)
                        .setStatus(resp.status)
                        .setBody(resp.body.string())

                for (h in resp.headers) {
                    rb.putHeaders(h.name, h.value)
                }
                rb
            } catch(e: Throwable) {
                MsgTypes.Response.newBuilder().setException(
                        MsgTypes.Response.ExceptionThrown.newBuilder()
                                .setName(e.javaClass.canonicalName)
                                .setMsg(e.message))
            }
            val built = rb.build()
            val needed = built.serializedSize
            val outputBuf = LibSupportFetchFFI.INSTANCE.support_fetch_alloc_bytebuffer(needed)
            try {
                // This is only null if we passed a negative number or something to
                // support_fetch_alloc_bytebuffer.
                val stream = outputBuf.asCodedOutputStream()!!
                built.writeTo(stream)
                return outputBuf
            } catch (e: Throwable) {
                // Note: we want to clean this up only if we are not returning it to rust.
                LibSupportFetchFFI.INSTANCE.support_fetch_destroy_bytebuffer(outputBuf)
                throw e
            }
        } finally {
            LibSupportFetchFFI.INSTANCE.support_fetch_destroy_bytebuffer(b)
        }
    }

}

internal fun convertMethod(m: MsgTypes.Request.Method): Request.Method {
    when (m) {
        MsgTypes.Request.Method.GET -> return Request.Method.GET
        MsgTypes.Request.Method.POST -> return Request.Method.POST
        MsgTypes.Request.Method.HEAD -> return Request.Method.HEAD
        MsgTypes.Request.Method.OPTIONS -> return Request.Method.OPTIONS
        MsgTypes.Request.Method.DELETE -> return Request.Method.DELETE
        MsgTypes.Request.Method.PUT -> return Request.Method.PUT
        MsgTypes.Request.Method.TRACE -> return Request.Method.TRACE
        MsgTypes.Request.Method.CONNECT -> return Request.Method.CONNECT
    }
}

internal class CallbackImpl : RawFetchCallback {
    override fun invoke(b: RustBuffer.ByValue): RustBuffer.ByValue {
        return RustHttpConfig.doFetch(b)
    }
}


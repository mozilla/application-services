/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.httpconfig

import com.google.protobuf.ByteString
import mozilla.appservices.support.RustBuffer
import mozilla.components.concept.fetch.Client
import mozilla.components.concept.fetch.MutableHeaders
import mozilla.components.concept.fetch.Request
import java.io.InputStream
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
        LibViaduct.INSTANCE.viaduct_initialize(imp!!)
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
                        Request.Body(ByteStringInputStream(request.body))
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
                        .setBody(resp.body.useStream {
                            ByteString.readFrom(it)
                        })

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
            val outputBuf = LibViaduct.INSTANCE.viaduct_alloc_bytebuffer(needed)
            try {
                // This is only null if we passed a negative number or something to
                // viaduct_alloc_bytebuffer.
                val stream = outputBuf.asCodedOutputStream()!!
                built.writeTo(stream)
                return outputBuf
            } catch (e: Throwable) {
                // Note: we want to clean this up only if we are not returning it to rust.
                LibViaduct.INSTANCE.viaduct_destroy_bytebuffer(outputBuf)
                throw e
            }
        } finally {
            LibViaduct.INSTANCE.viaduct_destroy_bytebuffer(b)
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

// The protobuf `bytes` type comes over as a com.google.protobuf.ByteString.
// There's no provided way to convert/wrap this to an InputStream, so we do
// that manually to avoid extra copying.
internal class ByteStringInputStream(private val s: ByteString) : InputStream() {
    private var pos: Int = 0

    override fun available(): Int {
        return s.size() - pos
    }

    override fun skip(n: Long): Long {
        val toSkip = Math.min((s.size() - pos).toLong(), Math.max(n, 0L)).toInt()
        pos += toSkip
        return toSkip.toLong()
    }

    override fun read(): Int {
        if (pos >= s.size()) {
            return -1
        }
        val result = s.byteAt(pos).toInt() and 0xff
        pos += 1
        return result
    }

    override fun read(bytes: ByteArray, off: Int, len: Int): Int {
        if (pos >= s.size()) {
            return -1
        }
        val toRead = Math.min(len, s.size() - pos)
        s.copyTo(bytes, pos, off, toRead)
        return toRead
    }

}

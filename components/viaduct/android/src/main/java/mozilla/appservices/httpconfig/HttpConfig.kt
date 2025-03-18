/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.httpconfig

import com.google.protobuf.ByteString
import mozilla.components.concept.fetch.Client
import mozilla.components.concept.fetch.MutableHeaders
import mozilla.components.concept.fetch.Request
import java.util.concurrent.TimeUnit
import java.util.concurrent.locks.ReentrantReadWriteLock
import kotlin.concurrent.read
import kotlin.concurrent.write

/**
 * All errors emitted by the client will subclass this.
 */
sealed class ViaductClientError(msg: String) : Exception(msg)

/**
 * Error indicating that the request method is not supported.
 */
class UnsupportedRequestMethodError(method: MsgTypes.Request.Method) : ViaductClientError("Unsupported HTTP method: $method")

/**
 * Singleton allowing management of the HTTP backend
 * used by Rust components.
 */
object RustHttpConfig {
    // Protects imp/client
    private var lock = ReentrantReadWriteLock()

    @Volatile
    private var client: Lazy<Client>? = null

    // Important note to future maintainers: if you mess around with
    // this code, you have to make sure `imp` can't get GCed. Extremely
    // bad things will happen if it does!
    @Volatile
    private var imp: CallbackImpl? = null

    /**
     * Set the HTTP client to be used by all Rust code.
     * the `Lazy`'s value is not read until the first request is made.
     */
    @Synchronized
    fun setClient(c: Lazy<Client>) {
        lock.write {
            client = c
            if (imp == null) {
                imp = CallbackImpl()
                LibViaduct.INSTANCE.viaduct_initialize(imp!!)
            }
        }
    }

    /** Allows connections to the hard-coded address the Android Emulator uses
     * to connect to the emulator's host (ie, http://10.0.2.2) - if you don't
     * call this, viaduct will fail to use that address as it isn't https. The
     * expectation is that you will only call this in debug builds or if you
     * are sure you are running on an emulator.
     */
    fun allowAndroidEmulatorLoopback() {
        lock.read {
            LibViaduct.INSTANCE.viaduct_allow_android_emulator_loopback()
        }
    }

    internal fun convertRequest(request: MsgTypes.Request): Request {
        val headers = MutableHeaders()
        for (h in request.headersMap) {
            headers.append(h.key, h.value)
        }
        return Request(
            url = request.url,
            method = convertMethod(request.method),
            headers = headers,
            connectTimeout = Pair(request.connectTimeoutSecs.toLong(), TimeUnit.SECONDS),
            readTimeout = Pair(request.readTimeoutSecs.toLong(), TimeUnit.SECONDS),
            body = if (request.hasBody()) {
                Request.Body(request.body.newInput())
            } else {
                null
            },
            redirect = if (request.followRedirects) {
                Request.Redirect.FOLLOW
            } else {
                Request.Redirect.MANUAL
            },
            cookiePolicy = Request.CookiePolicy.OMIT,
            useCaches = request.useCaches,
        )
    }

    @Suppress("TooGenericExceptionCaught", "ReturnCount")
    internal fun doFetch(b: RustBuffer.ByValue): RustBuffer.ByValue {
        lock.read {
            try {
                val request = MsgTypes.Request.parseFrom(b.asCodedInputStream())
                val rb = try {
                    // Note: `client!!` is fine here, since if client is null,
                    // we wouldn't have yet initialized
                    val resp = client!!.value.fetch(convertRequest(request))
                    val rb = MsgTypes.Response.newBuilder()
                        .setUrl(resp.url)
                        .setStatus(resp.status)
                        .setBody(
                            resp.body.useStream {
                                ByteString.readFrom(it)
                            },
                        )

                    for (h in resp.headers) {
                        rb.putHeaders(h.name, h.value)
                    }
                    rb
                } catch (e: Throwable) {
                    MsgTypes.Response.newBuilder().setExceptionMessage("fetch error: ${e.message ?: e.javaClass.canonicalName}")
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
                    LibViaduct.INSTANCE.viaduct_log_error("Failed to write buffer: ${e.message}")
                    throw e
                }
            } finally {
                LibViaduct.INSTANCE.viaduct_destroy_bytebuffer(b)
            }
        }
    }
}

internal fun convertMethod(m: MsgTypes.Request.Method): Request.Method {
    return when (m) {
        MsgTypes.Request.Method.GET -> Request.Method.GET
        MsgTypes.Request.Method.POST -> Request.Method.POST
        MsgTypes.Request.Method.HEAD -> Request.Method.HEAD
        MsgTypes.Request.Method.OPTIONS -> Request.Method.OPTIONS
        MsgTypes.Request.Method.DELETE -> Request.Method.DELETE
        MsgTypes.Request.Method.PUT -> Request.Method.PUT
        MsgTypes.Request.Method.TRACE -> Request.Method.TRACE
        MsgTypes.Request.Method.CONNECT -> Request.Method.CONNECT
        else -> throw UnsupportedRequestMethodError(m)
    }
}

internal class CallbackImpl : RawFetchCallback {
    @Suppress("TooGenericExceptionCaught")
    override fun invoke(b: RustBuffer.ByValue): RustBuffer.ByValue {
        try {
            return RustHttpConfig.doFetch(b)
        } catch (e: Throwable) {
            LibViaduct.INSTANCE.viaduct_log_error("doFetch failed: ${e.message}")
            // This is our last resort. It's bad news should we fail to
            // return something from this function.
            return RustBuffer.ByValue()
        }
    }
}

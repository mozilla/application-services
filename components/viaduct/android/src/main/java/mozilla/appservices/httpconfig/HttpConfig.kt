/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.httpconfig

import com.google.protobuf.ByteString
import mozilla.appservices.viaduct.initBackend
import mozilla.components.concept.fetch.Client
import mozilla.components.concept.fetch.MutableHeaders
import mozilla.components.concept.fetch.Request
import java.util.concurrent.TimeUnit
import kotlin.concurrent.atomics.AtomicBoolean
import kotlin.concurrent.atomics.ExperimentalAtomicApi
import mozilla.appservices.viaduct.allowAndroidEmulatorLoopback as rustAllowAndroidEmulatorLoopback

/**
 * All errors emitted by the client will subclass this.
 */
sealed class ViaductClientError(msg: String) : Exception(msg)

/**
 * Error indicating that the request method is not supported.
 */
class UnsupportedRequestMethodError(method: String) :
    ViaductClientError("Unsupported HTTP method: $method")

/**
 * Singleton allowing management of the HTTP backend
 * used by Rust components.
 */
@OptIn(ExperimentalAtomicApi::class)
object RustHttpConfig {
    // Used to only initialize the client once
    // https://bugzilla.mozilla.org/show_bug.cgi?id=1989865.
    private var backendInitialized = AtomicBoolean(false)

    /**
     * Set the HTTP client to be used by all Rust code.
     * the `Lazy`'s value is not read until the first request is made.
     */
    @Synchronized
    fun setClient(c: Lazy<Client>) {
        if (backendInitialized.compareAndSet(false, true)) {
            initBackend(FetchBackend(c))
        }
    }

    /** Allows connections to the hard-coded address the Android Emulator uses
     * to connect to the emulator's host (ie, http://10.0.2.2) - if you don't
     * call this, viaduct will fail to use that address as it isn't https. The
     * expectation is that you will only call this in debug builds or if you
     * are sure you are running on an emulator.
     */
    fun allowAndroidEmulatorLoopback() {
        rustAllowAndroidEmulatorLoopback()
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
        else -> throw UnsupportedRequestMethodError(m.toString())
    }
}

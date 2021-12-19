/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.httpconfig

import mozilla.appservices.httpconfig.uniffi.FfiBackend
import mozilla.appservices.httpconfig.uniffi.FfiRequest
import mozilla.appservices.httpconfig.uniffi.FfiResponse
import mozilla.appservices.httpconfig.uniffi.Fetcher
import mozilla.appservices.httpconfig.uniffi.Method
import mozilla.components.concept.fetch.Client
import mozilla.components.concept.fetch.MutableHeaders
import mozilla.components.concept.fetch.Request
import mozilla.components.concept.fetch.Response
import java.util.concurrent.TimeUnit

internal fun Method.into(): Request.Method {
    return when (this) {
        Method.GET -> Request.Method.GET
        Method.POST -> Request.Method.POST
        Method.HEAD -> Request.Method.HEAD
        Method.OPTIONS -> Request.Method.OPTIONS
        Method.DELETE -> Request.Method.DELETE
        Method.PUT -> Request.Method.PUT
        Method.TRACE -> Request.Method.TRACE
        Method.CONNECT -> Request.Method.CONNECT
    }
}

internal fun FfiRequest.into(): Request {
    val headers = MutableHeaders()
    for (h in this.headers) {
        headers.append(h.key, h.value)
    }
    return Request(
        url = this.url,
        method = this.method.into(),
        headers = headers,
        connectTimeout = Pair(this.connectTimeoutSecs.toLong(), TimeUnit.SECONDS),
        readTimeout = Pair(this.readTimeoutSecs.toLong(), TimeUnit.SECONDS),
        body = if (this.body != null) {
            Request.Body(this.body!!.toByteArray().inputStream())
        } else {
            null
        },
        redirect = if (this.followRedirects) {
            Request.Redirect.FOLLOW
        } else {
            Request.Redirect.MANUAL
        },
        cookiePolicy = Request.CookiePolicy.OMIT,
        useCaches = this.useCaches
    )
}

internal fun Response.into(requestMethod: Method): FfiResponse {
    val body = this.body.string().toByteArray().asList()
    val headers = mutableMapOf<String, String>()
    for (h in this.headers) {
        headers[h.name] = h.value
    }
    return FfiResponse.Ok(requestMethod, this.url, this.status, body, headers)
}

/**
 * Singleton allowing management of the HTTP backend
 * used by Rust components.
 */
object RustHttpConfig : Fetcher {
    @Volatile
    private var client: Lazy<Client>? = null

    /**
     * Set the HTTP client to be used by all Rust code.
     * the `Lazy`'s value is not read until the first request is made.
     * NOTE: This should only be set once per application lifetime
     *  otherwise it is a no-op
     */
    @Synchronized
    fun setClient(c: Lazy<Client>) {
        if (client == null) {
            client = c
            val ffiBackend = FfiBackend()
            ffiBackend.setBackend(this)
        }
    }

    /**
     * Important: This can run concurrently on multiple threads
     *  possibly at the same time. Do not introduce shared memory
     */
    @Suppress("TooGenericExceptionCaught")
    override fun fetch(request: FfiRequest): FfiResponse {
        // We don't need to lock here because if we end up here,
        // it means that `setClient` was already called and it
        // successfully registered this callback.
        // `setClient` guarantees that the `client` can't change
        // after it's set initially
        return try {
            // The `!!` is safe here because this callback can only be
            // called after the client is set
            client!!.value.fetch(request.into()).into(request.method)
        } catch (e: Throwable) {
            // Ideally, the catching of the error should happen in `uniffi`
            // generated code and it should be converted to a proper Rust Error
            // but until we add support for that, this should do
            FfiResponse.Err(message = e.message.orEmpty())
        }
    }
}

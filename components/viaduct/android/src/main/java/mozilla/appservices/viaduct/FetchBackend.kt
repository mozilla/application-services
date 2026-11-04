/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.viaduct

import mozilla.components.concept.fetch.Headers.Names.CONTENT_LENGTH
import java.io.ByteArrayOutputStream
import java.util.concurrent.TimeUnit
import mozilla.components.concept.fetch.Client as FetchClient
import mozilla.components.concept.fetch.Header as FetchHeader
import mozilla.components.concept.fetch.MutableHeaders as FetchMutableHeaders
import mozilla.components.concept.fetch.Request as FetchRequest

private const val DEFAULT_INITIAL_BUFFER_SIZE = 8 * 1024

internal class FetchBackend(val client: Lazy<FetchClient>) : Backend {
    override suspend fun sendRequest(request: Request, settings: ClientSettings): Response {
        val fetchReq = FetchRequest(
            url = request.url,
            method = when (request.method) {
                Method.GET -> FetchRequest.Method.GET
                Method.POST -> FetchRequest.Method.POST
                Method.HEAD -> FetchRequest.Method.HEAD
                Method.OPTIONS -> FetchRequest.Method.OPTIONS
                Method.DELETE -> FetchRequest.Method.DELETE
                Method.PUT -> FetchRequest.Method.PUT
                Method.TRACE -> FetchRequest.Method.TRACE
                Method.CONNECT -> FetchRequest.Method.CONNECT
                else -> throw UnsupportedRequestMethodError(request.method.toString())
            },
            headers = FetchMutableHeaders(
                request.headers.map { (name, value) ->
                    FetchHeader(name, value)
            },
            ),
            body = request.body.let {
                if (it != null) {
                    FetchRequest.Body(it.inputStream())
                } else {
                    null
                }
            },
            // Try to translate to the Fetch API as best we can
            readTimeout = if (settings.timeout > 0UL) {
                Pair(settings.timeout.toLong(), TimeUnit.MILLISECONDS)
            } else {
                null
            },
            redirect = if (settings.redirectLimit.toInt() > 0) {
                FetchRequest.Redirect.FOLLOW
            } else {
                FetchRequest.Redirect.MANUAL
            },
            cookiePolicy = FetchRequest.CookiePolicy.OMIT,
            useCaches = true,
        )
        val fetchResp = client.value.fetch(fetchReq)
        return Response(
            requestMethod = request.method,
            url = fetchResp.url,
            status = fetchResp.status.toUShort(),
            headers = fetchResp.headers
                .map { Pair(it.name, it.value) }
                .toMap(),
            body = fetchResp.body.useStream {
                // Use the content-length header as the initial size of our buffer.
                // If present and correct, this means we won't need to resize the buffer.
                //
                // Note: We're trusting the remote server to not pass us a content-length value
                // that's too large and will cause an OOM error.
                // We should probably set a max body size at some point.
                // However, that would need to happen for viaduct as a whole not this backend.
                val initialBufferSize = fetchResp.headers[CONTENT_LENGTH]
                    ?.trim()
                    ?.toIntOrNull()
                    ?.coerceIn(0, Int.MAX_VALUE)
                    ?: DEFAULT_INITIAL_BUFFER_SIZE
                val buffer = ByteArrayOutputStream(initialBufferSize)
                it.copyTo(buffer)
                buffer.toByteArray()
            },
        )
    }
}

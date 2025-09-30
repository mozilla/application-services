/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.httpconfig

import mozilla.appservices.viaduct.Backend
import mozilla.appservices.viaduct.ClientSettings
import mozilla.appservices.viaduct.Method
import mozilla.appservices.viaduct.Request
import mozilla.appservices.viaduct.Response
import java.util.concurrent.TimeUnit
import mozilla.components.concept.fetch.Client as FetchClient
import mozilla.components.concept.fetch.Header as FetchHeader
import mozilla.components.concept.fetch.MutableHeaders as FetchMutableHeaders
import mozilla.components.concept.fetch.Request as FetchRequest

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
                it.readBytes()
            },

        )
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

/**
 * Config represents the server endpoint configurations needed for the
 * authentication flow.
 */
class Config constructor(
    val contentUrl: String,
    val clientId: String,
    val redirectUri: String,
    val tokenServerUrlOverride: String? = null
) {
    enum class Server(val contentUrl: String) {
        RELEASE("https://accounts.firefox.com"),
        STABLE("https://stable.dev.lcip.org"),
        STAGE("https://accounts.stage.mozaws.net"),
        CHINA("https://accounts.firefox.com.cn"),
        LOCALDEV("http://127.0.0.1:3030")
    }

    constructor(
        server: Server,
        clientId: String,
        redirectUri: String,
        tokenServerUrlOverride: String? = null
    ) : this(server.contentUrl, clientId, redirectUri, tokenServerUrlOverride)
}

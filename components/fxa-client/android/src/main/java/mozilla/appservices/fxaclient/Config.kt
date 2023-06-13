/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

/**
 * Config represents the server endpoint configurations needed for the
 * authentication flow.
 */
class Config constructor(
    val server: FxaServer,
    val clientId: String,
    val redirectUri: String,
    val tokenServerUrlOverride: String? = null,
) {
    enum class Server(val rustServer: FxaServer) {
        RELEASE(FxaServer.Release),
        STABLE(FxaServer.Stable),
        STAGE(FxaServer.Stage),
        CHINA(FxaServer.China),
        LOCALDEV(FxaServer.LocalDev),
    }

    constructor(
        server: Server,
        clientId: String,
        redirectUri: String,
        tokenServerUrlOverride: String? = null,
    ) : this(server.rustServer, clientId, redirectUri, tokenServerUrlOverride)

    // Rust defines a config and server class that's virtually identically to these.  We should
    // remove the wrapper soon, but let's wait until we have a batch of breaking changes and do them
    // all at once.
    fun intoRustConfig() = FxaConfig(server, clientId, redirectUri, tokenServerUrlOverride)
}

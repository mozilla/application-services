/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.fxaclient.internal

/**
 * FirefoxAccount represents the authentication state of a client.
 */
class FirefoxAccount : RustObject {

    internal constructor(rawPointer: FxaHandle): super(rawPointer)

    /**
     * Create a FirefoxAccount using the given config.
     *
     * This does not make network requests, and can be used on the main thread.
     */
    constructor(config: Config)
    : this(unlockedRustCall { e ->
        FxaClient.INSTANCE.fxa_new(config.contentUrl, config.clientId, config.redirectUri, e)
    })

    override fun destroy(p: Long) {
        unlockedRustCall { err ->
            FxaClient.INSTANCE.fxa_free(p, err)
        }
    }

    /**
     * Constructs a URL used to begin the OAuth flow for the requested scopes and keys.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param scopes List of OAuth scopes for which the client wants access
     * @param wantsKeys Fetch keys for end-to-end encryption of data from Mozilla-hosted services
     * @return String that resolves to the flow URL when complete
     */
    fun beginOAuthFlow(scopes: Array<String>, wantsKeys: Boolean): String {
        val scope = scopes.joinToString(" ")
        return rustCall { e ->
            FxaClient.INSTANCE.fxa_begin_oauth_flow(validHandle(), scope, wantsKeys, e)
        }.getAndConsumeString()
    }

    /**
     * Begins the pairing flow.
     *
     * This performs network requests, and should not be used on the main thread.
     */
    fun beginPairingFlow(pairingUrl: String, scopes: Array<String>): String {
        val scope = scopes.joinToString(" ")
        return rustCall { e ->
            FxaClient.INSTANCE.fxa_begin_pairing_flow(validHandle(), pairingUrl, scope, e)
        }.getAndConsumeString()
    }

    /**
     * Fetches the profile object for the current client either from the existing cached account,
     * or from the server (requires the client to have access to the profile scope).
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param ignoreCache Fetch the profile information directly from the server
     * @return [Profile] representing the user's basic profile info
     * @throws FxaException.Unauthorized We couldn't find any suitable access token to make that call.
     * The caller should then start the OAuth Flow again with the "profile" scope.
     */
    fun getProfile(ignoreCache: Boolean): Profile {
        val profileBuffer = rustCall { e ->
            FxaClient.INSTANCE.fxa_profile(validHandle(), ignoreCache, e)
        }
        try {
            val p = MsgTypes.Profile.parseFrom(profileBuffer.asCodedInputStream()!!)
            return Profile.fromMessage(p)
        } finally {
            FxaClient.INSTANCE.fxa_bytebuffer_free(profileBuffer)
        }
    }

    /**
     * Convenience method to fetch the profile from a cached account by default, but fall back
     * to retrieval from the server.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @return [Profile] representing the user's basic profile info
     * @throws FxaException.Unauthorized We couldn't find any suitable access token to make that call.
     * The caller should then start the OAuth Flow again with the "profile" scope.
     */
    fun getProfile(): Profile {
        return getProfile(false)
    }

    /**
     * Fetches the token server endpoint, for authentication using the SAML bearer flow.
     *
     * This does not make network requests, and can be used on the main thread.
     */
    fun getTokenServerEndpointURL(): String {
        return rustCall { e ->
            FxaClient.INSTANCE.fxa_get_token_server_endpoint_url(validHandle(), e)
        }.getAndConsumeString()
    }

    /**
     * Fetches the connection success url.
     *
     * This does not make network requests, and can be used on the main thread.
     */
    fun getConnectionSuccessURL(): String {
        return rustCall { e ->
            FxaClient.INSTANCE.fxa_get_connection_success_url(validHandle(), e)
        }.getAndConsumeString()
    }

    /**
     * Authenticates the current account using the code and state parameters fetched from the
     * redirect URL reached after completing the sign in flow triggered by [beginOAuthFlow].
     *
     * Modifies the FirefoxAccount state.
     *
     * This performs network requests, and should not be used on the main thread.
     */
    fun completeOAuthFlow(code: String, state: String) {
        rustCall { e ->
            FxaClient.INSTANCE.fxa_complete_oauth_flow(validHandle(), code, state, e)
        }
    }

    /**
     * Tries to fetch an access token for the given scope.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param scope Single OAuth scope (no spaces) for which the client wants access
     * @return [AccessTokenInfo] that stores the token, along with its scopes and keys when complete
     * @throws FxaException.Unauthorized We couldn't provide an access token
     * for this scope. The caller should then start the OAuth Flow again with
     * the desired scope.
     */
    fun getAccessToken(scope: String): AccessTokenInfo {
        return AccessTokenInfo(rustCall { e ->
            FxaClient.INSTANCE.fxa_get_access_token(validHandle(), scope, e)
        })
    }

    /**
     * Saves the current account's authentication state as a JSON string, for persistence in
     * the Android KeyStore/shared preferences. The authentication state can be restored using
     * [FirefoxAccount.fromJSONString].
     *
     * This does not make network requests, and can be used on the main thread.
     *
     * @return String containing the authentication details in JSON format
     */
    fun toJSONString(): String {
        return rustCall { e ->
            FxaClient.INSTANCE.fxa_to_json(validHandle(), e)
        }.getAndConsumeString()
    }

    companion object {

        /**
         * Restores the account's authentication state from a JSON string produced by
         * [FirefoxAccount.toJSONString].
         *
         * This does not make network requests, and can be used on the main thread.
         *
         * @return [FirefoxAccount] representing the authentication state
         */
        fun fromJSONString(json: String): FirefoxAccount {
            return FirefoxAccount(unlockedRustCall { e ->
                FxaClient.INSTANCE.fxa_from_json(json, e)
            })
        }
    }
}

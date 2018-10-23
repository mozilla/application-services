/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package org.mozilla.fxaclient.internal

/**
 * FirefoxAccount represents the authentication state of a client.
 */
class FirefoxAccount : RustObject<RawFxAccount> {

    internal constructor(rawPointer: RawFxAccount): super(rawPointer)

    /**
     * Create a FirefoxAccount using the given config, client id, and redirect uri.
     *
     * If the config is passed into this method, calling `close` on it is no longer required
     * (but may be done if desired).
     *
     * This does not make network requests, and can be used on the main thread.
     */
    constructor(config: Config, clientId: String, redirectUri: String)
    : this(config.rustCall { e ->
        FxaClient.INSTANCE.fxa_new(config.consumePointer(), clientId, redirectUri, e)
    })

    override fun destroy(p: RawFxAccount) {
        // We're already synchronized by RustObject
        FxaClient.INSTANCE.fxa_free(p)
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
            FxaClient.INSTANCE.fxa_begin_oauth_flow(validPointer(), scope, wantsKeys, e)
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
            FxaClient.INSTANCE.fxa_begin_pairing_flow(validPointer(), pairingUrl, scope, e)
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
     */
    fun getProfile(ignoreCache: Boolean): Profile {
        return Profile(rustCall { e ->
            FxaClient.INSTANCE.fxa_profile(validPointer(), ignoreCache, e)
        })
    }

    /**
     * Convenience method to fetch the profile from a cached account by default, but fall back
     * to retrieval from the server.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @return [Profile] representing the user's basic profile info
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
            FxaClient.INSTANCE.fxa_get_token_server_endpoint_url(validPointer(), e)
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
    fun completeOAuthFlow(code: String, state: String): OAuthInfo {
        return OAuthInfo(rustCall { e ->
            FxaClient.INSTANCE.fxa_complete_oauth_flow(validPointer(), code, state, e)
        })
    }

    /**
     * Tries to fetch a cached access token for the given scope.
     *
     * If the token is close to expiration, we may refresh it.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param scopes List of OAuth scopes for which the client wants access
     * @return [OAuthInfo] that stores the token, along with its scopes and keys when complete
     */
    fun getCachedOAuthToken(scopes: Array<String>): OAuthInfo? {
        val scope = scopes.joinToString(" ")
        return nullableRustCall { e ->
            FxaClient.INSTANCE.fxa_get_oauth_token(validPointer(), scope, e)
        }?.let { OAuthInfo(it) }
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
            FxaClient.INSTANCE.fxa_to_json(validPointer(), e)
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

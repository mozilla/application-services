/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

import android.util.Log
import com.sun.jna.Pointer
import mozilla.appservices.fxaclient.rust.FxaHandle
import mozilla.appservices.fxaclient.rust.LibFxAFFI
import mozilla.appservices.fxaclient.rust.RustError
import java.util.concurrent.atomic.AtomicLong

/**
 * FirefoxAccount represents the authentication state of a client.
 */
class FirefoxAccount(handle: FxaHandle, persistCallback: PersistCallback?) : AutoCloseable {
    private var handle: AtomicLong = AtomicLong(handle)
    private var persistCallback: PersistCallback? = persistCallback

    /**
     * Create a FirefoxAccount using the given config.
     *
     * This does not make network requests, and can be used on the main thread.
     */
    constructor(config: Config, persistCallback: PersistCallback? = null) :
    this(rustCall { e ->
        LibFxAFFI.INSTANCE.fxa_new(config.contentUrl, config.clientId, config.redirectUri, e)
    }, persistCallback) {
        // Persist the newly created instance state.
        this.tryPersistState()
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
        fun fromJSONString(json: String, persistCallback: PersistCallback? = null): FirefoxAccount {
            return FirefoxAccount(rustCall { e ->
                LibFxAFFI.INSTANCE.fxa_from_json(json, e)
            }, persistCallback)
        }
    }

    interface PersistCallback {
        fun persist(data: String)
    }

    /**
     * Registers a PersistCallback that will be called every time the
     * FirefoxAccount internal state has mutated.
     * The FirefoxAccount instance can be later restored using the
     * `fromJSONString` class method.
     * It is the responsibility of the consumer to ensure the persisted data
     * is saved in a secure location, as it can contain Sync Keys and
     * OAuth tokens.
     */
    fun registerPersistCallback(persistCallback: PersistCallback) {
        this.persistCallback = persistCallback
    }

    /**
     * Unregisters any previously registered PersistCallback.
     */
    fun unregisterPersistCallback() {
        this.persistCallback = null
    }

    private fun tryPersistState() {
        this.persistCallback?.let {
            val json: String
            try {
                json = this.toJSONString()
            } catch (e: FxaException) {
                Log.e("FirefoxAccount", "Error serializing the FirefoxAccount state.")
                return
            }
            it.persist(json)
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
        return rustCallWithLock { e ->
            LibFxAFFI.INSTANCE.fxa_begin_oauth_flow(this.handle.get(), scope, wantsKeys, e)
        }.getAndConsumeRustString()
    }

    /**
     * Begins the pairing flow.
     *
     * This performs network requests, and should not be used on the main thread.
     */
    fun beginPairingFlow(pairingUrl: String, scopes: Array<String>): String {
        val scope = scopes.joinToString(" ")
        return rustCallWithLock { e ->
            LibFxAFFI.INSTANCE.fxa_begin_pairing_flow(this.handle.get(), pairingUrl, scope, e)
        }.getAndConsumeRustString()
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
        rustCallWithLock { e ->
            LibFxAFFI.INSTANCE.fxa_complete_oauth_flow(this.handle.get(), code, state, e)
        }
        this.tryPersistState()
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
        val profileBuffer = rustCallWithLock { e ->
            LibFxAFFI.INSTANCE.fxa_profile(this.handle.get(), ignoreCache, e)
        }
        try {
            val p = MsgTypes.Profile.parseFrom(profileBuffer.asCodedInputStream()!!)
            return Profile.fromMessage(p)
        } finally {
            LibFxAFFI.INSTANCE.fxa_bytebuffer_free(profileBuffer)
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
        return rustCallWithLock { e ->
            LibFxAFFI.INSTANCE.fxa_get_token_server_endpoint_url(this.handle.get(), e)
        }.getAndConsumeRustString()
    }

    /**
     * Fetches the connection success url.
     *
     * This does not make network requests, and can be used on the main thread.
     */
    fun getConnectionSuccessURL(): String {
        return rustCallWithLock { e ->
            LibFxAFFI.INSTANCE.fxa_get_connection_success_url(this.handle.get(), e)
        }.getAndConsumeRustString()
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
        val buffer = rustCallWithLock { e ->
            LibFxAFFI.INSTANCE.fxa_get_access_token(this.handle.get(), scope, e)
        }
        try {
            val msg = MsgTypes.AccessTokenInfo.parseFrom(buffer.asCodedInputStream()!!)
            return AccessTokenInfo.fromMessage(msg)
        } finally {
            LibFxAFFI.INSTANCE.fxa_bytebuffer_free(buffer)
        }
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
        return rustCallWithLock { e ->
            LibFxAFFI.INSTANCE.fxa_to_json(this.handle.get(), e)
        }.getAndConsumeRustString()
    }

    @Synchronized
    override fun close() {
        val handle = this.handle.getAndSet(0)
        if (handle != 0L) {
            rustCall { err ->
                LibFxAFFI.INSTANCE.fxa_free(handle, err)
            }
        }
    }

    private inline fun <U> nullableRustCallWithLock(callback: (RustError.ByReference) -> U?): U? {
        return synchronized(this) {
            nullableRustCall { callback(it) }
        }
    }

    private inline fun <U> rustCallWithLock(callback: (RustError.ByReference) -> U?): U {
        return nullableRustCallWithLock(callback)!!
    }
}

// In practice we usually need to be synchronized to call this safely, so it doesn't
// synchronize itself
private inline fun <U> nullableRustCall(callback: (RustError.ByReference) -> U?): U? {
    val e = RustError.ByReference()
    try {
        val ret = callback(e)
        if (e.isFailure()) {
            throw e.intoException()
        }
        return ret
    } finally {
        // This only matters if `callback` throws (or does a non-local return, which
        // we currently don't do)
        e.ensureConsumed()
    }
}

private inline fun <U> rustCall(callback: (RustError.ByReference) -> U?): U {
    return nullableRustCall(callback)!!
}

/**
 * Helper to read a null terminated String out of the Pointer and free it.
 *
 * Important: Do not use this pointer after this! For anything!
 */
internal fun Pointer.getAndConsumeRustString(): String {
    try {
        return this.getRustString()
    } finally {
        LibFxAFFI.INSTANCE.fxa_str_free(this)
    }
}

/**
 * Helper to read a null terminated string out of the pointer.
 *
 * Important: doesn't free the pointer, use [getAndConsumeRustString] for that!
 */
internal fun Pointer.getRustString(): String {
    return this.getString(0, "utf8")
}

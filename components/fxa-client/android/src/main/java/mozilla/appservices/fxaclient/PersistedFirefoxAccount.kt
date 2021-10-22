/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

import android.util.Log
import org.json.JSONObject

class PersistedFxADecorator(
    private var persistCallback: PersistedFirefoxAccount.PersistCallback?
) :  FxADecorator<FirefoxAccount>, FxAPersistCallbackHolder {
    override fun <ReturnType> thenPersistsState(obj: FirefoxAccount, thunk: () -> ReturnType) =
        try {
            thunk()
        } finally {
            tryPersistState(obj)
        }

    override fun tryPersistState(obj: FirefoxAccountInterface) {
        this.persistCallback?.let {
            val json: String
            try {
                json = obj.toJson()
            } catch (e: FxaException) {
                Log.e("FirefoxAccount", "Error serializing the FirefoxAccount state.")
                return
            }
            it.persist(json)
        }
    }

    override fun registerPersistCallback(persistCallback: PersistedFirefoxAccount.PersistCallback) {
        this.persistCallback = persistCallback
    }

    override fun unregisterPersistCallback() {
        this.persistCallback = null
    }
}

interface FxAPersistCallbackHolder {
    /**
     * Registers a PersistCallback that will be called every time the
     * FirefoxAccount internal state has mutated.
     * The FirefoxAccount instance can be later restored using the
     * `fromJSONString` class method.
     * It is the responsibility of the consumer to ensure the persisted data
     * is saved in a secure location, as it can contain Sync Keys and
     * OAuth tokens.
     */
    fun registerPersistCallback(persistCallback: PersistedFirefoxAccount.PersistCallback)

    /**
     * Unregisters any previously registered PersistCallback.
     */
    fun unregisterPersistCallback()

    fun tryPersistState(obj: FirefoxAccountInterface)
}

/**
 * PersistedFirefoxAccount represents the authentication state of a client.
 *
 * This is a thin wrapper around the `FirefoxAccount` object exposed from Rust.
 * Its main job is to transparently manage persistence of the account state by
 * calling the provided callback at appropriate times.
 *
 * In future it would be nice to push this logic down into the Rust code,
 * once UniFFI's support for callback interfaces is a little more battle-tested.
 *
 */
class PersistedFirefoxAccount
    private constructor(private val inner: FirefoxAccount) : AutoCloseable,
        FirefoxAccountInterface by inner,
        FxAPersistCallbackHolder by (inner.fxADecorator as PersistedFxADecorator)
    {

    /**
     * Create a PersistedFirefoxAccount using the given config.
     *
     * This does not make network requests, and can be used on the main thread.
     *
     */
    constructor(config: Config, persistCallback: PersistCallback? = null) : this(FirefoxAccount(
        PersistedFxADecorator(persistCallback),
        // This is kind of dumb - we take a Config object on the Kotlin side, destructure it into its fields
        // to pass over the FFI, then the Rust side turns it back into its own variant of a Config object!
        // That made sense when we had to write the FFI layer by hand, but we should see whether we can nicely
        // expose the Rust Config interface to Kotlin and Swift and then just accept a Config here in the
        // underlying `FirefoxAccount` constructor.
        config.contentUrl,
        config.clientId,
        config.redirectUri,
        config.tokenServerUrlOverride
    )) {
        // Persist the newly created instance state.
        tryPersistState(this)
    }

    companion object {
        /**
         * Restores the account's authentication state from a JSON string produced by
         * [PersistedFirefoxAccount.toJSONString].
         *
         * This does not make network requests, and can be used on the main thread.
         *
         * @return [PersistedFirefoxAccount] representing the authentication state
         */
        fun fromJSONString(json: String, persistCallback: PersistCallback? = null): PersistedFirefoxAccount {
            return PersistedFirefoxAccount(FirefoxAccount.fromJson(PersistedFxADecorator(persistCallback), json))
        }
    }

    interface PersistCallback {
        fun persist(data: String)
    }


    /**
     * Constructs a URL used to begin the OAuth flow for the requested scopes and keys.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param scopes List of OAuth scopes for which the client wants access
     * @param entrypoint to be used for metrics
     * @param metricsParams optional parameters used for metrics
     * @return String that resolves to the flow URL when complete
     */
    fun beginOAuthFlow(
        scopes: List<String>,
        entrypoint: String
    ) = this.beginOauthFlow(scopes.toList(), entrypoint, MetricsParams(mapOf()))

    /**
     * Begins the pairing flow.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param pairingUrl the url to initilaize the paring flow with
     * @param scopes List of OAuth scopes for which the client wants access
     * @param entrypoint to be used for metrics
     * @param metricsParams optional parameters used for metrics
     * @return String that resoles to the flow URL when complete
     */
    fun beginPairingFlow(
        pairingUrl: String,
        scopes: Array<String>,
        entrypoint: String
    ) = this.beginPairingFlow(pairingUrl, scopes.toList(), entrypoint, MetricsParams(mapOf()))

    /**
     * Provisions an OAuth code using the session token from state
     *
     * @param authParams Parameters needed for the authorization request
     * This performs network requests, and should not be used on the main thread.
     */
    fun authorizeOAuthCode(
        authParams: AuthorizationParameters
    ) = this.authorizeCodeUsingSessionToken(authParams)

    /**
     * Migrate from a logged-in Firefox Account, takes ownership of the provided session token.
     *
     * Modifies the FirefoxAccount state.
     * @param sessionToken 64 character string of hex-encoded bytes
     * @param kSync 128 character string of hex-encoded bytes
     * @param kXCS 32 character string of hex-encoded bytes
     * @return JSONObject JSON object with the result of the migration
     * This performs network requests, and should not be used on the main thread.
     */
    fun migrateFromSessionTokenToJSON(sessionToken: String, kSync: String, kXCS: String): JSONObject {
        val res = this.migrateFromSessionToken(sessionToken, kSync, kXCS, false)
        return JSONObject(mapOf("total_duration" to res.totalDuration))
    }

    /**
     * Copy a logged-in session of a Firefox Account, creates a new session token in the process.
     *
     * Modifies the FirefoxAccount state.
     * @param sessionToken 64 character string of hex-encoded bytes
     * @param kSync 128 character string of hex-encoded bytes
     * @param kXCS 32 character string of hex-encoded bytes
     * @return JSONObject JSON object with the result of the migration
     * This performs network requests, and should not be used on the main thread.
     */
    fun copyFromSessionToken(sessionToken: String, kSync: String, kXCS: String): JSONObject {
        val res = this.migrateFromSessionToken(sessionToken, kSync, kXCS, true)
        return JSONObject(mapOf("total_duration" to res.totalDuration))
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
        return this.toJson()
    }

    /**
     * Update the push subscription details for the current device.
     * This needs to be called every time a push subscription is modified or expires.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param endpoint Push callback URL
     * @param publicKey Public key used to encrypt push payloads
     * @param authKey Auth key used to encrypt push payloads
     */
    fun setDevicePushSubscription(endpoint: String, publicKey: String, authKey: String) =
        this.setPushSubscription(DevicePushSubscription(endpoint, publicKey, authKey))

    /**
     * Update the display name (as shown in the FxA device manager, or the Send Tab target list)
     * for the current device.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param displayName The current device display name
     */
    fun setDeviceDisplayName(displayName: String) = setDeviceName(displayName)

    /**
     * Ensure the current device is registered with the specified name and device type, with
     * the required capabilities (at this time only Send Tab).
     * This method should be called once per "device lifetime".
     *
     * This performs network requests, and should not be used on the main thread.
     */
    fun initializeDevice(name: String, deviceType: DeviceType, supportedCapabilities: Set<DeviceCapability>) =
        this.initializeDevice(name, deviceType, supportedCapabilities.toList())

    @Synchronized
    override fun close() {
        inner.destroy()
    }
}

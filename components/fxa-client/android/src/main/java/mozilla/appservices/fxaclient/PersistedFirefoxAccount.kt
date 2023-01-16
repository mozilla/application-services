/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

import android.util.Log
import org.json.JSONObject

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
class PersistedFirefoxAccount(inner: FirefoxAccount, persistCallback: PersistCallback?) : AutoCloseable {
    private var inner: FirefoxAccount = inner
    private var persistCallback: PersistCallback? = persistCallback

    /**
     * Create a PersistedFirefoxAccount using the given config.
     *
     * This does not make network requests, and can be used on the main thread.
     *
     */
    constructor(config: Config, persistCallback: PersistCallback? = null) : this(FirefoxAccount(
        // This is kind of dumb - we take a Config object on the Kotlin side, destructure it into its fields
        // to pass over the FFI, then the Rust side turns it back into its own variant of a Config object!
        // That made sense when we had to write the FFI layer by hand, but we should see whether we can nicely
        // expose the Rust Config interface to Kotlin and Swift and then just accept a Config here in the
        // underlying `FirefoxAccount` constructor.
        config.contentUrl,
        config.clientId,
        config.redirectUri,
        config.tokenServerUrlOverride
    ), persistCallback) {
        // Persist the newly created instance state.
        this.tryPersistState()
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
            return PersistedFirefoxAccount(FirefoxAccount.fromJson(json), persistCallback)
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
     * @param entrypoint to be used for metrics
     * @param metricsParams optional parameters used for metrics
     * @return String that resolves to the flow URL when complete
     */
    fun beginOAuthFlow(
        scopes: Array<String>,
        entrypoint: String,
        metricsParams: MetricsParams = MetricsParams(mapOf())
    ): String {
        return this.inner.beginOauthFlow(scopes.toList(), entrypoint, metricsParams)
    }

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
        entrypoint: String,
        metricsParams: MetricsParams = MetricsParams(mapOf())
    ): String {
        return this.inner.beginPairingFlow(pairingUrl, scopes.toList(), entrypoint, metricsParams)
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
        this.inner.completeOauthFlow(code, state)
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
        try {
            return this.inner.getProfile(ignoreCache)
        } finally {
            this.tryPersistState()
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
     * Fetches the token server endpoint, for authenticating to Firefox Sync via OAuth.
     *
     * This performs network requests, and should not be used on the main thread.
     */
    fun getTokenServerEndpointURL(): String {
        return this.inner.getTokenServerEndpointUrl()
    }

    /**
     * Get the pairing URL to navigate to on the Auth side (typically a computer).
     *
     * This does not make network requests, and can be used on the main thread.
     */
    fun getPairingAuthorityURL(): String {
        return this.inner.getPairingAuthorityUrl()
    }

    /**
     * Fetches the connection success url.
     *
     * This does not make network requests, and can be used on the main thread.
     */
    fun getConnectionSuccessURL(): String {
        return this.inner.getConnectionSuccessUrl()
    }

    /**
     * Fetches the user's manage-account url.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @throws FxaException.Unauthorized We couldn't find any suitable access token to identify the user.
     * The caller should then start the OAuth Flow again with the "profile" scope.
     */
    fun getManageAccountURL(entrypoint: String): String {
        return this.inner.getManageAccountUrl(entrypoint)
    }

    /**
     * Fetches the user's manage-devices url.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @throws FxaException.Unauthorized We couldn't find any suitable access token to identify the user.
     * The caller should then start the OAuth Flow again with the "profile" scope.
     */
    fun getManageDevicesURL(entrypoint: String): String {
        return this.inner.getManageDevicesUrl(entrypoint)
    }

    /**
     * Tries to fetch an access token for the given scope.
     *
     * This performs network requests, and should not be used on the main thread.
     * It may modify the persisted account state.
     *
     * @param scope Single OAuth scope (no spaces) for which the client wants access
     * @param ttl time in seconds for which the token will be valid
     * @return [AccessTokenInfo] that stores the token, along with its scopes and keys when complete
     * @throws FxaException.Unauthorized We couldn't provide an access token
     * for this scope. The caller should then start the OAuth Flow again with
     * the desired scope.
     */
    fun getAccessToken(scope: String, ttl: Long? = null): AccessTokenInfo {
        try {
            return this.inner.getAccessToken(scope, ttl)
        } finally {
            this.tryPersistState()
        }
    }

    fun checkAuthorizationStatus(): AuthorizationInfo {
        return this.inner.checkAuthorizationStatus()
    }

    /**
     * Tries to return a session token
     *
     * @throws FxaException Will send you an exception if there is no session token set
     */
    fun getSessionToken(): String {
        return this.inner.getSessionToken()
    }

    /**
     * Get the current device id
     *
     * @throws FxaException Will send you an exception if there is no device id set
     */
    fun getCurrentDeviceId(): String {
        return this.inner.getCurrentDeviceId()
    }

    /**
     * Provisions an OAuth code using the session token from state
     *
     * @param authParams Parameters needed for the authorization request
     * This performs network requests, and should not be used on the main thread.
     */
    fun authorizeOAuthCode(
        authParams: AuthorizationParameters
    ): String {
        return this.inner.authorizeCodeUsingSessionToken(authParams)
    }

    /**
     * This method should be called when a request made with
     * an OAuth token failed with an authentication error.
     * It clears the internal cache of OAuth access tokens,
     * so the caller can try to call `getAccessToken` or `getProfile`
     * again.
     */
    fun clearAccessTokenCache() {
        this.inner.clearAccessTokenCache()
    }

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
    fun migrateFromSessionToken(sessionToken: String, kSync: String, kXCS: String): JSONObject {
        try {
            val res = this.inner.migrateFromSessionToken(sessionToken, kSync, kXCS, false)
            return JSONObject(mapOf("total_duration" to res.totalDuration))
        } finally {
            // Even a failed migration might alter the persisted account state, if it's able to be retried.
            // It's safe to call this unconditionally, as the underlying code will not leave partial states.
            this.tryPersistState()
        }
    }

    /**
     * Migrate from a logged-in Firefox Account, takes ownership of the provided session token.
     *
     * @return bool Returns a boolean if we are in a migration state
     */
    fun isInMigrationState(): MigrationState {
        return this.inner.isInMigrationState()
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
        try {
            val res = this.inner.migrateFromSessionToken(sessionToken, kSync, kXCS, true)
            return JSONObject(mapOf("total_duration" to res.totalDuration))
        } finally {
            // Even a failed migration might alter the persisted account state, if it's able to be retried.
            // It's safe to call this unconditionally, as the underlying code will not leave partial states.
            this.tryPersistState()
        }
    }

    /**
     * Retry migration from a logged-in Firefox Account.
     *
     * Modifies the FirefoxAccount state.
     * @return JSONObject JSON object with the result of the migration
     * This performs network requests, and should not be used on the main thread.
     */
    fun retryMigrateFromSessionToken(): JSONObject {
        try {
            val res = this.inner.retryMigrateFromSessionToken()
            return JSONObject(mapOf("total_duration" to res.totalDuration))
        } finally {
            // A failure her might alter the persisted account state, if we discover a permanent migration failure.
            // It's safe to call this unconditionally, as the underlying code will not leave partial states.
            this.tryPersistState()
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
        return this.inner.toJson()
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
    fun setDevicePushSubscription(endpoint: String, publicKey: String, authKey: String) {
        try {
            return this.inner.setPushSubscription(DevicePushSubscription(endpoint, publicKey, authKey))
        } finally {
            this.tryPersistState()
        }
    }

    /**
     * Update the display name (as shown in the FxA device manager, or the Send Tab target list)
     * for the current device.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param displayName The current device display name
     */
    fun setDeviceDisplayName(displayName: String) {
        try {
            return this.inner.setDeviceName(displayName)
        } finally {
            this.tryPersistState()
        }
    }

    /**
     * Retrieves the list of the connected devices in the current account, including the current one.
     *
     * This performs network requests, and should not be used on the main thread.
     */
    fun getDevices(ignoreCache: Boolean = false): Array<Device> {
        return this.inner.getDevices(ignoreCache).toTypedArray()
    }

    /**
     * Disconnect from the account and optionaly destroy our device record.
     * `beginOAuthFlow` will need to be called to reconnect.
     *
     * This performs network requests, and should not be used on the main thread.
     */
    fun disconnect() {
        this.inner.disconnect()
        this.tryPersistState()
    }

    /**
     * Retrieves any pending commands for the current device.
     * This should be called semi-regularly as the main method of commands delivery (push)
     * can sometimes be unreliable on mobile devices.
     * If a persist callback is set and the host application failed to process the
     * returned account events, they will never be seen again.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @return A collection of [IncomingDeviceCommand] that should be handled by the caller.
     */
    fun pollDeviceCommands(): Array<IncomingDeviceCommand> {
        try {
            return this.inner.pollDeviceCommands().toTypedArray()
        } finally {
            this.tryPersistState()
        }
    }

    /**
     * Handle any incoming push message payload coming from the Firefox Accounts
     * servers that has been decrypted and authenticated by the Push crate.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @return A collection of [AccountEvent] that should be handled by the caller.
     */
    fun handlePushMessage(payload: String): Array<AccountEvent> {
        try {
            return this.inner.handlePushMessage(payload).toTypedArray()
        } finally {
            this.tryPersistState()
        }
    }

    /**
     * Ensure the current device is registered with the specified name and device type, with
     * the required capabilities (at this time only Send Tab).
     * This method should be called once per "device lifetime".
     *
     * This performs network requests, and should not be used on the main thread.
     */
    fun initializeDevice(name: String, deviceType: DeviceType, supportedCapabilities: Set<DeviceCapability>) {
        this.inner.initializeDevice(name, deviceType, supportedCapabilities.toList())
        this.tryPersistState()
    }

    /**
     * Ensure that the supported capabilities described earlier in `initializeDevice` are A-OK.
     * A set of capabilities to be supported by the Device must also be passed (at this time only
     * Send Tab).
     *
     * As for now there's only the Send Tab capability, so we ensure the command is registered with the server.
     * This method should be called at least every time the sync keys change (because Send Tab relies on them).
     *
     * This performs network requests, and should not be used on the main thread.
     */
    fun ensureCapabilities(supportedCapabilities: Set<DeviceCapability>) {
        this.inner.ensureCapabilities(supportedCapabilities.toList())
        this.tryPersistState()
    }

    /**
     * Send a single tab to another device identified by its device ID.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param targetDeviceId The target Device ID
     * @param title The document title of the tab being sent
     * @param url The url of the tab being sent
     */
    fun sendSingleTab(targetDeviceId: String, title: String, url: String) {
        this.inner.sendSingleTab(targetDeviceId, title, url)
    }

    /**
     * Gather any telemetry which has been collected internally and return
     * the result as a JSON string.
     *
     * This does not make network requests, and can be used on the main thread.
     */
    fun gatherTelemetry(): String {
        return this.inner.gatherTelemetry()
    }

    fun registerEventHandler(handler: FirefoxAccountEventHandler) {
        this.inner.registerEventHandler(handler)
    }

    fun unregisterEventHandler() {
        this.inner.unregisterEventHandler()
    }

    @Synchronized
    override fun close() {
        this.inner.destroy()
    }
}

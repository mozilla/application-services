/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

import android.util.Log
import mozilla.appservices.sync15.DeviceType
import org.mozilla.appservices.fxaclient.GleanMetrics.FxaClient as FxaClientMetrics

/**
 * FxaClient represents the authentication state of a client.
 *
 * This is a thin wrapper around the `FirefoxAccount` object exposed from Rust.
 * Its main job is to transparently manage persistence of the account state by
 * calling the provided callback at appropriate times.
 *
 * In future it would be nice to push this logic down into the Rust code,
 * once UniFFI's support for callback interfaces is a little more battle-tested.
 *
 */
class FxaClient(inner: FirefoxAccount, persistCallback: PersistCallback?) : AutoCloseable {
    private var inner: FirefoxAccount = inner
    private var persistCallback: PersistCallback? = persistCallback

    /**
     * Create a FxaClient using the given config.
     *
     * This does not make network requests, and can be used on the main thread.
     *
     */
    constructor(config: FxaConfig, persistCallback: PersistCallback? = null) : this(
        FirefoxAccount(config),
        persistCallback,
    ) {
        // Persist the newly created instance state.
        this.tryPersistState()
    }

    companion object {
        /**
         * Restores the account's authentication state from a JSON string produced by
         * [FxaClient.toJSONString].
         *
         * This does not make network requests, and can be used on the main thread.
         *
         * @return [FxaClient] representing the authentication state
         */
        fun fromJSONString(json: String, persistCallback: PersistCallback? = null): FxaClient {
            return FxaClient(FirefoxAccount.fromJson(json), persistCallback)
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
     * Process an event (login, logout, etc).
     *
     * On success, update the current state and return it.
     * On error, the current state will remain the same.
     */
    fun processEvent(event: FxaEvent): FxaState = this.inner.processEvent(event)

    /**
     * Get the high-level authentication state of the client
     */
    fun getAuthState() = this.inner.getAuthState()

    /**
     * Constructs a URL used to begin the OAuth flow for the requested scopes and keys.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param scopes List of OAuth scopes for which the client wants access
     * @param entrypoint to be used for metrics
     * @return String that resolves to the flow URL when complete
     */
    fun beginOAuthFlow(
        scopes: Array<String>,
        entrypoint: String,
    ): String {
        return withMetrics {
            this.inner.beginOauthFlow(scopes.toList(), entrypoint)
        }
    }

    /**
     * Begins the pairing flow.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param pairingUrl the url to initilaize the paring flow with
     * @param scopes List of OAuth scopes for which the client wants access
     * @param entrypoint to be used for metrics
     * @return String that resoles to the flow URL when complete
     */
    fun beginPairingFlow(
        pairingUrl: String,
        scopes: Array<String>,
        entrypoint: String,
    ): String {
        return withMetrics {
            this.inner.beginPairingFlow(pairingUrl, scopes.toList(), entrypoint)
        }
    }

    /**
     * Sets user data from the web content.
     * NOTE: this is only useful for applications that are user agents
     *       and require the user's session token
     * @param userData: The user data including session token, email and uid
     */
    fun setUserData(
        userData: UserData,
    ) {
        this.inner.setUserData(userData)
        tryPersistState()
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
        withMetrics {
            this.inner.completeOauthFlow(code, state)
            this.tryPersistState()
        }
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
        return withMetrics {
            try {
                this.inner.getProfile(ignoreCache)
            } finally {
                this.tryPersistState()
            }
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
        return withMetrics {
            getProfile(false)
        }
    }

    /**
     * Fetches the token server endpoint, for authenticating to Firefox Sync via OAuth.
     *
     * This performs network requests, and should not be used on the main thread.
     */
    fun getTokenServerEndpointURL(): String {
        return withMetrics {
            this.inner.getTokenServerEndpointUrl()
        }
    }

    /**
     * Get the pairing URL to navigate to on the Auth side (typically a computer).
     *
     * This does not make network requests, and can be used on the main thread.
     */
    fun getPairingAuthorityURL(): String {
        return withMetrics {
            this.inner.getPairingAuthorityUrl()
        }
    }

    /**
     * Fetches the connection success url.
     *
     * This does not make network requests, and can be used on the main thread.
     */
    fun getConnectionSuccessURL(): String {
        return withMetrics {
            this.inner.getConnectionSuccessUrl()
        }
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
        return withMetrics {
            this.inner.getManageAccountUrl(entrypoint)
        }
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
        return withMetrics {
            this.inner.getManageDevicesUrl(entrypoint)
        }
    }

    /**
     * Tries to fetch an access token for the given scope.
     *
     * This performs network requests, and should not be used on the main thread.
     * It may modify the persisted account state.
     *
     * On `FxaException.Unauthorized` and `FxaException.SyncScopedKeyMissingInServerResponse`, the
     * caller should indicate to the user that there are authentication issues and allow them to
     * re-login by starting a new OAuth flow.
     *
     * @param scope Single OAuth scope (no spaces) for which the client wants access
     * @param ttl time in seconds for which the token will be valid
     * @return [AccessTokenInfo] that stores the token, along with its scopes and keys when complete
     * @throws FxaException.Network Network error while requesting the access token.
     * @throws FxaException.Unauthorized We couldn't provide an access token for this scope.
     * @throws FxaException.SyncScopedKeyMissingInServerResponse we received an access token for the
     * sync scoped, but the sync key that should accompany it was missing.
     */
    fun getAccessToken(scope: String, ttl: Long? = null): AccessTokenInfo {
        return withMetrics {
            try {
                this.inner.getAccessToken(scope, ttl)
            } finally {
                this.tryPersistState()
            }
        }
    }

    fun checkAuthorizationStatus(): AuthorizationInfo {
        return withMetrics {
            this.inner.checkAuthorizationStatus()
        }
    }

    /**
     * Tries to return a session token
     *
     * @throws FxaException Will send you an exception if there is no session token set
     */
    fun getSessionToken(): String {
        return withMetrics {
            this.inner.getSessionToken()
        }
    }

    /**
     * Get the current device id
     *
     * @throws FxaException Will send you an exception if there is no device id set
     */
    fun getCurrentDeviceId(): String {
        return withMetrics {
            this.inner.getCurrentDeviceId()
        }
    }

    /**
     * Provisions an OAuth code using the session token from state
     *
     * @param authParams Parameters needed for the authorization request
     * This performs network requests, and should not be used on the main thread.
     */
    fun authorizeOAuthCode(
        authParams: AuthorizationParameters,
    ): String {
        return withMetrics {
            this.inner.authorizeCodeUsingSessionToken(authParams)
        }
    }

    /**
     * This method should be called when a request made with
     * an OAuth token failed with an authentication error.
     * It clears the internal cache of OAuth access tokens,
     * so the caller can try to call `getAccessToken` or `getProfile`
     * again.
     */
    fun clearAccessTokenCache() {
        withMetrics {
            this.inner.clearAccessTokenCache()
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
        return withMetrics {
            try {
                this.inner.setPushSubscription(DevicePushSubscription(endpoint, publicKey, authKey))
            } finally {
                this.tryPersistState()
            }
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
        return withMetrics {
            try {
                this.inner.setDeviceName(displayName)
            } finally {
                this.tryPersistState()
            }
        }
    }

    /**
     * Retrieves the list of the connected devices in the current account, including the current one.
     *
     * This performs network requests, and should not be used on the main thread.
     */
    fun getDevices(ignoreCache: Boolean = false): Array<Device> {
        return withMetrics {
            this.inner.getDevices(ignoreCache).toTypedArray()
        }
    }

    /**
     * Disconnect from the account and optionally destroy our device record.
     * `beginOAuthFlow` will need to be called to reconnect.
     *
     * This performs network requests, and should not be used on the main thread.
     */
    fun disconnect() {
        withMetrics {
            this.inner.disconnect()
            this.tryPersistState()
        }
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
        return withMetrics {
            try {
                this.inner.pollDeviceCommands().toTypedArray()
            } finally {
                this.tryPersistState()
            }
        }
    }

    /**
     * Retrieves the account event associated with an
     * incoming push message payload coming Firefox Accounts.
     * Assumes the message that has been decrypted and authenticated by the Push crate.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @return A collection of [AccountEvent] that should be handled by the caller.
     */
    fun handlePushMessage(payload: String): AccountEvent {
        return withMetrics {
            try {
                this.inner.handlePushMessage(payload)
            } finally {
                this.tryPersistState()
            }
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
        withMetrics {
            this.inner.initializeDevice(name, deviceType, supportedCapabilities.toList())
            this.tryPersistState()
        }
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
        withMetrics {
            this.inner.ensureCapabilities(supportedCapabilities.toList())
            this.tryPersistState()
        }
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
        withMetrics {
            this.inner.sendSingleTab(targetDeviceId, title, url)
        }
    }

    /**
     * Close one or more tabs that are currently open on another device.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param targetDeviceId The ID of the device on which the tabs are
     * currently open.
     * @param urls The URLs of the tabs to close.
     * @return The result of the operation.
     */
    fun closeTabs(targetDeviceId: String, urls: List<String>): CloseTabsResult =
        withMetrics {
            this.inner.closeTabs(targetDeviceId, urls)
        }

    /**
     * Gather any telemetry which has been collected internally and return
     * the result as a JSON string.
     *
     * This does not make network requests, and can be used on the main thread.
     */
    fun gatherTelemetry(): String {
        return withMetrics {
            this.inner.gatherTelemetry()
        }
    }

    /**
     * Perform an FxA operation and gather metrics on it
     */
    @Suppress("ThrowsCount", "TooGenericExceptionCaught")
    fun<T> withMetrics(operation: () -> T): T {
        return try {
            FxaClientMetrics.operationCount.add()
            operation()
        } catch (e: FxaException.Network) {
            FxaClientMetrics.errorCount["network"].add()
            throw e
        } catch (e: FxaException.Authentication) {
            FxaClientMetrics.errorCount["authentication"].add()
            throw e
        } catch (e: FxaException.NoExistingAuthFlow) {
            FxaClientMetrics.errorCount["no_existing_auth_flow"].add()
            throw e
        } catch (e: FxaException.OriginMismatch) {
            FxaClientMetrics.errorCount["origin_mismatch"].add()
            throw e
        } catch (e: FxaException) {
            FxaClientMetrics.errorCount["fxa_other"].add()
            throw e
        } catch (e: Throwable) {
            FxaClientMetrics.errorCount["unexpected"].add()
            throw e
        }
    }

    /**
     * Used by the application to test auth token issues
     */
    fun simulateNetworkError() = this.inner.simulateNetworkError()

    /**
     * Used by the application to test auth token issues
     */
    fun simulateTemporaryAuthTokenIssue() = this.inner.simulateTemporaryAuthTokenIssue()

    /**
     * Used by the application to test auth token issues
     */
    fun simulatePermanentAuthTokenIssue() = this.inner.simulatePermanentAuthTokenIssue()

    @Synchronized
    override fun close() {
        this.inner.destroy()
    }
}

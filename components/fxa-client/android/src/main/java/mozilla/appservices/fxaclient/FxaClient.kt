/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.withContext
import kotlin.coroutines.CoroutineContext
import org.mozilla.appservices.fxaclient.GleanMetrics.FxaClient as FxaClientMetrics

// This allows `adb logcat | grep fxa_client` to pick up all log items from this component
internal const val LOG_TAG = "fxa_client"

/**
 * Firefox account client
 */
class FxaClient private constructor(
    private val inner: FirefoxAccount,
    private val persistCallback: FxaPersistCallback,
    // Note that this creates a reference cycle between FxaAccountManager, FxaClient, and FxaHandler
    // that goes through the Rust code and therefore will never be broken. This is okay for now,
    // since all of those objects are intended to live forever, but hopefully we can add some sort
    // of weak reference support to side-step this.
    private val eventHandler: FxaEventHandler,
    coroutineContext: CoroutineContext,
) : AutoCloseable {
    /**
     * CoroutineContext context to run async tasks in.
     *
     * This is the coroutineContext passed in to the constructor plus a SupervisorJob.  The
     * SupervisorJob ensures that if one task fails the other still run.
     *
     * Because we use a single CoroutineContext, [close] can cancel all active jobs.
     *
     * This is public so applications can use the same context to run their jobs in.  For example
     * the android-components FxaManager uses it for its startup tasks.
     */
    public val coroutineContext = coroutineContext + SupervisorJob()

    /**
     * Create a new FxaClient
     *
     * @param config FxaConfig to initialize the client with
     * @param FxaEventHandler Respond to FxA events
     * @param coroutineContext CoroutineContext for the client.
     */
    constructor(config: FxaConfig, persistCallback: FxaPersistCallback, eventHandler: FxaEventHandler, coroutineContext: CoroutineContext = Dispatchers.IO) : this(
        FirefoxAccount(config),
        persistCallback,
        eventHandler,
        coroutineContext,
    )

    companion object {
        /**
         * Restores a perisisted FxaClient
         *
         * @param json JSON data sent to FxaEventHandler.persistData
         * @param FxaEventHandler Respond to FxA events
         * @param coroutineContext CoroutineContext for the client.
         * @return [FxaClient] representing the authentication state
         */
        fun fromJson(json: String, persistCallback: FxaPersistCallback, eventHandler: FxaEventHandler, coroutineContext: CoroutineContext = Dispatchers.IO): FxaClient {
            return FxaClient(
                FirefoxAccount.fromJson(json),
                persistCallback,
                eventHandler,
                coroutineContext,
            )
        }
    }

    private fun persistState() {
        @Suppress("TooGenericExceptionCaught")
        try {
            persistCallback.persist(inner.toJson())
        } catch (e: Exception) {
            Log.e(LOG_TAG, "Error saving the FirefoxAccount state.")
        }
    }

    /**
     * Queue an FxaAction for processing and immediately return
     *
     * Use the FxaEventHandler passed to the constructor to respond to the results of queued
     * actions.
     *
     * Why do this?
     *   - Actions are processed serially.  For example, there's no chance of CompleteOAuthFlow and
     *     Disconnect being executed at the same time from different threads.
     *   - Application events are also sent serially. If one action causes an [AUTH_CHECK_STARTED]
     *     state transition change, then the next causes [FxaAuthState.Disconnected], the
     *     [FxaEventHandler.onStateChange] callback for the second change won't be called until
     *     after the callback for the first change returns. This allows applications to ensure that
     *     their UI reflects the current state.
     *   - Actions are retried in the face of network errors and checkAuthorizationStatus is called on
     *     authorization errors.  This allows the Rust client to recover when possible, for example
     *     from expired access tokens when it holds a valid refresh token.
     */
    fun queueAction(action: FxaAction) {
        actionProcessor.queue(action)
    }

    // This handles queueAction for us
    private val actionProcessor = FxaActionProcessor(inner, eventHandler, { persistState() })
    init {
        runActionProcessorManager(actionProcessor, coroutineContext)
    }

    /**
     * Get the current authentication state
     *
     * FIXME: https://github.com/mozilla/application-services/issues/5819
     */
    fun getAuthState(): FxaAuthState = FxaAuthState.fromRust(inner.getAuthState())

    // Wraps method calls that don't change the state, like [getAccessToken]
    private suspend fun<T> wrapMethodCall(name: String, methodCall: () -> T): T {
        Log.d(LOG_TAG, "Running: $name")
        return withContext(coroutineContext) {
            try {
                withMetrics {
                    methodCall()
                }
            } catch (e: FxaException.Authentication) {
                queueAction(FxaAction.CheckAuthorization)
                throw e
            }
        }
    }

    // Does what wrapMethodCall does and also ensures that the state is perisisted at the end of the
    // operation.
    private suspend fun<T> wrapMethodCallAndPersist(name: String, methodCall: () -> T): T {
        return try {
            wrapMethodCall(name, methodCall)
        } finally {
            // Use a finally block since we want to perisist the state regardless of if the method
            // succeeded or not.
            persistState()
        }
    }

    /**
     * Fetches the profile object for the current client either from the existing cached account,
     * or from the server (requires the client to have access to the profile scope).
     *
     * @param ignoreCache Fetch the profile information directly from the server
     * @return [Profile] representing the user's basic profile info
     * @throws FxaException.Unauthorized We couldn't find any suitable access token to make that call.
     * The caller should then start the OAuth Flow again with the "profile" scope.
     */
    suspend fun getProfile(ignoreCache: Boolean): Profile = wrapMethodCallAndPersist("getProfile") {
        inner.getProfile(ignoreCache)
    }

    /**
     * Convenience method to fetch the profile from a cached account by default, but fall back
     * to retrieval from the server.
     *
     * @return [Profile] representing the user's basic profile info
     * @throws FxaException.Unauthorized We couldn't find any suitable access token to make that call.
     * The caller should then start the OAuth Flow again with the "profile" scope.
     */
    suspend fun getProfile(): Profile = getProfile(false)

    /**
     * Fetches the token server endpoint, for authenticating to Firefox Sync via OAuth.
     */
    suspend fun getTokenServerEndpointURL(): String = wrapMethodCall("getTokenServerEndpointURL") {
        inner.getTokenServerEndpointUrl()
    }

    /**
     * Get the pairing URL to navigate to on the Auth side (typically a computer).
     *
     * FIXME: https://github.com/mozilla/application-services/issues/5819
     */
    fun getPairingAuthorityURL(): String = withMetrics {
        inner.getPairingAuthorityUrl()
    }

    /**
     * Fetches the connection success url.
     *
     * FIXME: https://github.com/mozilla/application-services/issues/5819
     */
    fun getConnectionSuccessURL(): String = withMetrics {
        inner.getConnectionSuccessUrl()
    }

    /**
     * Fetches the user's manage-account url.
     *
     * @throws FxaException.Unauthorized We couldn't find any suitable access token to identify the user.
     * The caller should then start the OAuth Flow again with the "profile" scope.
     */
    suspend fun getManageAccountURL(entrypoint: String): String = wrapMethodCall("getManageAccountURL") {
        inner.getManageAccountUrl(entrypoint)
    }

    /**
     * Fetches the user's manage-devices url.
     *
     * @throws FxaException.Unauthorized We couldn't find any suitable access token to identify the user.
     * The caller should then start the OAuth Flow again with the "profile" scope.
     */
    suspend fun getManageDevicesURL(entrypoint: String): String = wrapMethodCall("getManageDevicesURL") {
        inner.getManageDevicesUrl(entrypoint)
    }

    /**
     * Tries to fetch an access token for the given scope.
     *
     * @param scope Single OAuth scope (no spaces) for which the client wants access
     * @param ttl time in seconds for which the token will be valid
     * @return [AccessTokenInfo] that stores the token, along with its scopes and keys when complete
     * @throws FxaException.Unauthorized We couldn't provide an access token
     * for this scope. The caller should then start the OAuth Flow again with
     * the desired scope.
     */
    suspend fun getAccessToken(
        scope: String,
        ttl: Long? = null,
        requireScopedKey: Boolean = false,
    ): AccessTokenInfo = wrapMethodCallAndPersist("getAccessToken") {
        inner.getAccessToken(scope, ttl, requireScopedKey)
    }

    /**
     * Tries to return a session token
     *
     * FIXME: https://github.com/mozilla/application-services/issues/5819
     *
     * @throws FxaException Will send you an exception if there is no session token set
     */
    fun getSessionToken(): String = withMetrics {
        inner.getSessionToken()
    }

    /**
     * Get the current device id
     *
     * FIXME: https://github.com/mozilla/application-services/issues/5819
     *
     * @throws FxaException Will send you an exception if there is no device id set
     */
    fun getCurrentDeviceId(): String = withMetrics {
        inner.getCurrentDeviceId()
    }

    /**
     * Provisions an OAuth code using the session token from state
     *
     * @param authParams Parameters needed for the authorization request
     * This performs network requests, and should not be used on the main thread.
     */
    suspend fun authorizeOAuthCode(
        authParams: AuthorizationParameters,
    ): String = wrapMethodCall("authorizeOAuthCode") {
        inner.authorizeCodeUsingSessionToken(authParams)
    }

    /**
     * This method should be called when a request made with
     * an OAuth token failed with an authentication error.
     * It clears the internal cache of OAuth access tokens,
     * so the caller can try to call [getAccessToken] or [getProfile]
     * again.
     *
     * FIXME: https://github.com/mozilla/application-services/issues/5819
     */
    fun clearAccessTokenCache() = withMetrics {
        inner.clearAccessTokenCache()
    }

    /**
     * Saves the current account's authentication state as a JSON string, for persistence in
     * the Android KeyStore/shared preferences. The authentication state can be restored using
     * [FirefoxAccount.fromJSONString].
     *
     * FIXME: https://github.com/mozilla/application-services/issues/5819
     *
     * @return String containing the authentication details in JSON format
     */
    fun toJsonString(): String {
        return inner.toJson()
    }

    /**
     * Retrieves the list of the connected devices in the current account, including the current one.
     */
    suspend fun getDevices(ignoreCache: Boolean = false): Array<Device> = wrapMethodCallAndPersist("getDevices") {
        inner.getDevices(ignoreCache).toTypedArray()
    }

    /**
     * Retrieves any pending commands for the current device.
     * This should be called semi-regularly as the main method of commands delivery (push)
     * can sometimes be unreliable on mobile devices.
     * If a persist callback is set and the host application failed to process the
     * returned account events, they will never be seen again.
     *
     * @return A collection of [IncomingDeviceCommand] that should be handled by the caller.
     */
    suspend fun pollDeviceCommands(): Array<IncomingDeviceCommand> = wrapMethodCallAndPersist("pollDeviceCommands") {
        inner.pollDeviceCommands().toTypedArray()
    }

    /**
     * Retrieves the account event associated with an
     * incoming push message payload coming Firefox Accounts.
     * Assumes the message that has been decrypted and authenticated by the Push crate.
     *
     * @return A collection of [AccountEvent] that should be handled by the caller.
     */
    suspend fun handlePushMessage(payload: String): AccountEvent = wrapMethodCallAndPersist("handlePushMessage") {
        inner.handlePushMessage(payload)
    }

    /**
     * Gather any telemetry which has been collected internally and return
     * the result as a JSON string.
     */
    suspend fun gatherTelemetry(): String = wrapMethodCall("gatherTelemetry") {
        inner.gatherTelemetry()
    }

    @Synchronized
    override fun close() {
        this.actionProcessor.close()
        this.coroutineContext.cancel()
        this.inner.destroy()
    }

    /**
     * Constructs a URL used to begin the OAuth flow for the requested scopes and keys.
     *
     * Deprecated: Call `queueAction(FxaAction.BeginOAuthFlow(...))` instead.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param scopes List of OAuth scopes for which the client wants access
     * @param entrypoint to be used for metrics
     * @param metricsParams optional parameters used for metrics
     * @return String that resolves to the flow URL when complete
     */
    @Deprecated("Send beginOAuthFlow to queueAction instead")
    fun beginOAuthFlow(
        scopes: Array<String>,
        entrypoint: String,
        metricsParams: MetricsParams = MetricsParams(mapOf()),
    ): String = withMetrics {
        this.inner.beginOauthFlow(scopes.toList(), entrypoint, metricsParams)
    }

    /**
     * Begins the pairing flow.
     *
     * Deprecated: Call `queueAction(FxaAction.BeginPairingFlow(...))` instead.
     *
     * This performs network requests, and should not be used on the main thread.
     *
     * @param pairingUrl the url to initilaize the paring flow with
     * @param scopes List of OAuth scopes for which the client wants access
     * @param entrypoint to be used for metrics
     * @param metricsParams optional parameters used for metrics
     * @return String that resoles to the flow URL when complete
     */
    @Deprecated("Send beginPairingFlow to queueAction instead")
    fun beginPairingFlow(
        pairingUrl: String,
        scopes: Array<String>,
        entrypoint: String,
        metricsParams: MetricsParams = MetricsParams(mapOf()),
    ): String = withMetrics {
        this.inner.beginPairingFlow(pairingUrl, scopes.toList(), entrypoint, metricsParams)
    }

    /**
     * Authenticates the current account using the code and state parameters fetched from the
     * redirect URL reached after completing the sign in flow triggered by [beginOAuthFlow].
     *
     * Deprecated: Call `queueAction(FxaAction.CompleteOAuthFlow(...))` instead.
     *
     * Modifies the FirefoxAccount state.
     *
     * This performs network requests, and should not be used on the main thread.
     */
    @Deprecated("Send CompleteOAuthFlow to queueAction instead")
    fun completeOAuthFlow(code: String, state: String) = withMetrics {
        this.inner.completeOauthFlow(code, state)
        this.persistState()
    }

    // These are used to test error handling in real applications, for example in Firefox Android
    // with the secret debug menu
    public fun simulateNetworkError() {
        Log.w(LOG_TAG, "simulateNetworkError")
        actionProcessor.simulateNetworkErrorFlag = true
    }

    public fun simulateTemporaryAuthTokenIssue() {
        Log.w(LOG_TAG, "simulateTemporaryAuthTokenIssue")
        inner.simulateTemporaryAuthTokenIssue()
    }

    public fun simulatePermanentAuthTokenIssue() {
        Log.w(LOG_TAG, "simulatePermanentAuthTokenIssue")
        inner.simulatePermanentAuthTokenIssue()
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
    } catch (e: FxaException) {
        FxaClientMetrics.errorCount["fxa_other"].add()
        throw e
    } catch (e: Throwable) {
        FxaClientMetrics.errorCount["unexpected"].add()
        throw e
    }
}

/**
 * Implemented by applications to save the Fxa state
 */
interface FxaPersistCallback {
    fun persist(data: String)
}

/**
 * Implemented by applications to respond to Fxa events
 */
interface FxaEventHandler {
    suspend fun onFxaEvent(event: FxaEvent)
}

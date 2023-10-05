/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

import android.util.Log
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlin.coroutines.CoroutineContext
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds
import org.mozilla.appservices.fxaclient.GleanMetrics.FxaClient as FxaClientMetrics

// How many items should our channel hold?  This should be big enough that it will never be filled
// under normal circumstances, but small enough that it will get filled if the `processChannel()`
// task fails (and therefore we will see exceptions when we call `trySend` rather than failing
// silently).
private const val CHANNEL_SIZE = 100

// How many times should we retry in the face of network errors?
private const val NETWORK_RETRY_MAX = 3

// How often should we try to recover from auth issues?
private val AUTH_RECOVERY_PERIOD = 60.seconds

/**
 * Processes actions sent to [FxaClient.queueAction] and sends events to the [FxaEventHandler]
 *
 * See [FxaClient.queueAction] for details on why we handle actions like this.
 *
 * [FxaActionProcessor] is "inert" on construction and won't process any events.  Call
 * [runActionProcessorManager] start event processing.  This creates a manager task that monitors
 * the action handling loop and restarts it if it throws for any reason.
 *
 */
internal class FxaActionProcessor(
    private val inner: FirefoxAccount,
    private val eventHandler: FxaEventHandler,
    private val persistState: () -> Unit,
    initialState: FxaAuthState = FxaAuthState.fromRust(inner.getAuthState()),
) {
    // Set a high bound on the channel so that if our the processChannel() task dies, or is never
    // started, we should eventually see error reports.
    private val channel = Channel<FxaAction>(CHANNEL_SIZE)
    internal val retryLogic = RetryLogic()
    internal var currentState = initialState

    @Volatile
    internal var simulateNetworkErrorFlag: Boolean = false

    // Queue a new action for processing
    fun queue(action: FxaAction) {
        channel.trySend(action)
    }

    // Shutdown the ActionProcessor
    fun close() {
        channel.close()
    }

    internal suspend fun processChannel() {
        Log.d(LOG_TAG, "processChannel started: $currentState")
        for (action in channel) {
            Log.d(LOG_TAG, "Processing action: $action")
            // Note: If this fails, runActionProcessorManager will catch the error and restart the
            // entire loop.
            processAction(action)
            Log.d(LOG_TAG, "Action processed: $action")
        }
        // Exiting the for loop means the channel is closed, which means we are closed.
        // Time to quit.
    }

    @Suppress("ComplexMethod")
    internal suspend fun processAction(action: FxaAction) {
        retryLogic.newActionStarted()
        val isValid = when (action) {
            // Auth flow actions are valid if you're disconnected and also if you're already
            // authenticating.  If a consumer accidentally starts multiple flows we should not
            // create extra issues for them.
            is FxaAction.BeginOAuthFlow,
            is FxaAction.BeginPairingFlow,
            is FxaAction.CompleteOAuthFlow,
            is FxaAction.CancelOAuthFlow,
            -> currentState in listOf(FxaAuthState.DISCONNECTED, FxaAuthState.AUTHENTICATING)
            // These actions require the user to be connected
            FxaAction.CheckAuthorization,
            is FxaAction.InitializeDevice,
            is FxaAction.EnsureCapabilities,
            is FxaAction.SetDeviceName,
            is FxaAction.SetDevicePushSubscription,
            is FxaAction.SendSingleTab,
            -> currentState.isConnected()
            // These are always valid, although they're no-op if you're already in the
            // DISCONNECTED/AUTH_ISSUES state
            FxaAction.Disconnect,
            FxaAction.LogoutFromAuthIssues,
            -> true
        }
        if (isValid) {
            FxaClientMetrics.operationCount.add()
            try {
                when (action) {
                    is FxaAction.BeginOAuthFlow -> handleBeginOAuthFlow(action)
                    is FxaAction.BeginPairingFlow -> handleBeginPairingFlow(action)
                    is FxaAction.CompleteOAuthFlow -> handleCompleteFlow(action)
                    is FxaAction.CancelOAuthFlow -> handleCancelFlow()
                    is FxaAction.InitializeDevice -> handleInitializeDevice(action)
                    is FxaAction.EnsureCapabilities -> handleEnsureCapabilities(action)
                    is FxaAction.SetDeviceName -> handleSetDeviceName(action)
                    is FxaAction.SetDevicePushSubscription -> handleSetDevicePushSubscription(action)
                    is FxaAction.SendSingleTab -> handleSendSingleTab(action)
                    FxaAction.CheckAuthorization -> handleCheckAuthorization()
                    FxaAction.Disconnect -> handleDisconnect()
                    FxaAction.LogoutFromAuthIssues -> handleLogoutFromAuthIssues()
                }
            } catch (e: FxaException) {
                FxaClientMetrics.errorCount["fxa_other"].add()
                throw e
            } catch (@Suppress("TooGenericExceptionCaught")e: Throwable) {
                FxaClientMetrics.errorCount["unexpected"].add()
                throw e
            }
        } else {
            Log.e(LOG_TAG, "Invalid $action (state: $currentState)")
        }
    }

    internal suspend fun sendEvent(event: FxaEvent) {
        Log.d(LOG_TAG, "Sending $event")
        @Suppress("TooGenericExceptionCaught")
        try {
            eventHandler.onFxaEvent(event)
        } catch (e: Exception) {
            Log.e(LOG_TAG, "Exception sending $event", e)
        }
    }

    private suspend fun sendAuthEvent(kind: FxaAuthEventKind, newState: FxaAuthState?) {
        if (newState != null && newState != currentState) {
            Log.d(LOG_TAG, "Changing state from $currentState to $newState")
            currentState = newState
        }
        sendEvent(FxaEvent.AuthEvent(kind, currentState))
    }

    // Perform an operation, retrying after network errors
    private suspend fun<T> withNetworkRetry(operation: suspend () -> T): T {
        while (true) {
            try {
                if (simulateNetworkErrorFlag) {
                    simulateNetworkErrorFlag = false
                    throw FxaException.Network("Simulated Error")
                }
                return operation()
            } catch (e: FxaException.Network) {
                FxaClientMetrics.errorCount["network"].add()
                if (retryLogic.shouldRetryAfterNetworkError()) {
                    Log.d(LOG_TAG, "Network error: retrying")
                    continue
                } else {
                    Log.d(LOG_TAG, "Network error: not retrying")
                    throw e
                }
            }
        }
    }

    // Perform an operation, retrying after network errors and calling checkAuthorizationStatus
    // after auth errors
    private suspend fun<T> withRetry(operation: suspend () -> T): T {
        while (true) {
            try {
                return withNetworkRetry(operation)
            } catch (e: FxaException.Authentication) {
                FxaClientMetrics.errorCount["authentication"].add()
                if (!currentState.isConnected()) {
                    throw e
                }

                if (retryLogic.shouldRecheckAuthStatus()) {
                    Log.d(LOG_TAG, "Auth error: re-checking")
                    handleCheckAuthorization()
                } else {
                    Log.d(LOG_TAG, "Auth error: disconnecting")
                    inner.logoutFromAuthIssues()
                    persistState()
                    sendAuthEvent(FxaAuthEventKind.AUTH_CHECK_FAILED, FxaAuthState.AUTH_ISSUES)
                }

                if (currentState.isConnected()) {
                    continue
                } else {
                    throw e
                }
            }
        }
    }

    private suspend fun handleBeginOAuthFlow(action: FxaAction.BeginOAuthFlow) {
        handleBeginEitherOAuthFlow(action.result) {
            inner.beginOauthFlow(action.scopes.toList(), action.entrypoint, action.metrics ?: MetricsParams(mapOf()))
        }
    }

    private suspend fun handleBeginPairingFlow(action: FxaAction.BeginPairingFlow) {
        handleBeginEitherOAuthFlow(action.result) {
            inner.beginPairingFlow(action.pairingUrl, action.scopes.toList(), action.entrypoint, action.metrics ?: MetricsParams(mapOf()))
        }
    }

    private suspend fun handleBeginEitherOAuthFlow(result: CompletableDeferred<String?>?, operation: () -> String) {
        try {
            val url = withRetry { operation() }
            persistState()
            sendAuthEvent(FxaAuthEventKind.OAUTH_STARTED, FxaAuthState.AUTHENTICATING)
            sendEvent(FxaEvent.BeginOAuthFlow(url))
            result?.complete(url)
        } catch (e: FxaException) {
            FxaClientMetrics.errorCount["fxa_other"].add()
            Log.e(LOG_TAG, "Exception when handling BeginOAuthFlow", e)
            persistState()
            // Stay in the AUTHENTICATING if we were in that state , since there may be another
            // oauth flow in progress.  We only switch to DISCONNECTED if we see CancelOAuthFlow.
            sendAuthEvent(FxaAuthEventKind.OAUTH_FAILED_TO_BEGIN, null)
            result?.complete(null)
        }
    }

    private suspend fun handleCompleteFlow(action: FxaAction.CompleteOAuthFlow) {
        try {
            withRetry { inner.completeOauthFlow(action.code, action.state) }
            persistState()
            sendAuthEvent(FxaAuthEventKind.OAUTH_COMPLETE, FxaAuthState.CONNECTED)
        } catch (e: FxaException) {
            FxaClientMetrics.errorCount["fxa_other"].add()
            persistState()
            Log.e(LOG_TAG, "Exception when handling CompleteOAuthFlow", e)
            // Stay in the AUTHENTICATING, since there may be another oauth flow in progress.  We
            // only switch to DISCONNECTED if we see CancelOAuthFlow.
            sendAuthEvent(FxaAuthEventKind.OAUTH_FAILED_TO_COMPLETE, null)
        }
    }

    private suspend fun handleCancelFlow() {
        // No need to call an inner method or persist the state, since the connecting flag is
        // handled in this layer only.
        sendAuthEvent(FxaAuthEventKind.OAUTH_CANCELLED, FxaAuthState.DISCONNECTED)
    }

    private suspend fun handleInitializeDevice(action: FxaAction.InitializeDevice) {
        handleDeviceOperation(action, FxaDeviceOperation.INITIALIZE_DEVICE, action.result) {
            withRetry { inner.initializeDevice(action.name, action.deviceType, action.supportedCapabilities) }
        }
    }

    private suspend fun handleEnsureCapabilities(action: FxaAction.EnsureCapabilities) {
        handleDeviceOperation(action, FxaDeviceOperation.ENSURE_CAPABILITIES, action.result) {
            withRetry { inner.ensureCapabilities(action.supportedCapabilities) }
        }
    }

    private suspend fun handleSetDeviceName(action: FxaAction.SetDeviceName) {
        handleDeviceOperation(action, FxaDeviceOperation.SET_DEVICE_NAME, action.result) {
            withRetry { inner.setDeviceName(action.displayName) }
        }
    }

    private suspend fun handleSetDevicePushSubscription(action: FxaAction.SetDevicePushSubscription) {
        handleDeviceOperation(action, FxaDeviceOperation.SET_DEVICE_PUSH_SUBSCRIPTION, action.result) {
            withRetry { inner.setPushSubscription(DevicePushSubscription(action.endpoint, action.publicKey, action.authKey)) }
        }
    }

    private suspend fun handleSendSingleTab(action: FxaAction.SendSingleTab) {
        try {
            withRetry { inner.sendSingleTab(action.targetDeviceId, action.title, action.url) }
            action.result?.complete(true)
        } catch (e: FxaException) {
            FxaClientMetrics.errorCount["fxa_other"].add()
            Log.e(LOG_TAG, "Exception when handling $action", e)
            action.result?.complete(false)
        }
    }

    private suspend fun handleDeviceOperation(
        action: FxaAction,
        operation: FxaDeviceOperation,
        result: CompletableDeferred<Boolean>?,
        block: suspend () -> LocalDevice,
    ) {
        try {
            val localDevice = block()
            sendEvent(FxaEvent.DeviceOperationComplete(operation, localDevice))
            result?.complete(true)
        } catch (e: FxaException) {
            FxaClientMetrics.errorCount["fxa_other"].add()
            Log.e(LOG_TAG, "Exception when handling $action", e)
            sendEvent(FxaEvent.DeviceOperationFailed(operation))
            result?.complete(false)
        }
    }

    private suspend fun handleDisconnect() {
        if (currentState == FxaAuthState.DISCONNECTED) {
            return
        }
        inner.disconnect()
        persistState()
        sendAuthEvent(FxaAuthEventKind.DISCONNECTED, FxaAuthState.DISCONNECTED)
    }

    private suspend fun handleLogoutFromAuthIssues() {
        if (currentState in listOf(FxaAuthState.AUTH_ISSUES, FxaAuthState.DISCONNECTED)) {
            return
        }
        inner.logoutFromAuthIssues()
        persistState()
        sendAuthEvent(FxaAuthEventKind.LOGOUT_FROM_AUTH_ISSUES, FxaAuthState.AUTH_ISSUES)
    }

    private suspend fun handleCheckAuthorization() {
        if (currentState in listOf(FxaAuthState.DISCONNECTED, FxaAuthState.AUTH_ISSUES)) {
            return
        }
        sendAuthEvent(FxaAuthEventKind.AUTH_CHECK_STARTED, FxaAuthState.CHECKING_AUTH)
        val success = try {
            val status = withNetworkRetry { inner.checkAuthorizationStatus() }
            status.active
        } catch (e: FxaException.Authentication) {
            FxaClientMetrics.errorCount["authorization"].add()
            // The Rust code should handle this exception, but if it doesn't, let's consider it a
            // failed check.
            Log.e(LOG_TAG, "Authentication exception when checking authorization status", e)
            false
        } catch (e: FxaException) {
            FxaClientMetrics.errorCount["fxa_other"].add()
            Log.e(LOG_TAG, "Exception when checking authorization status", e)
            // It's not clear what's the Right Thing to do in this case, but considering it a success is
            // better than a failure.  We don't want users logged out because of network issues.
            true
        }
        if (success) {
            persistState()
            sendAuthEvent(FxaAuthEventKind.AUTH_CHECK_SUCCESS, FxaAuthState.CONNECTED)
        } else {
            inner.logoutFromAuthIssues()
            persistState()
            sendAuthEvent(FxaAuthEventKind.AUTH_CHECK_FAILED, FxaAuthState.AUTH_ISSUES)
        }
    }
}

internal class RetryLogic {
    private var networkRetryCount = 0
    private var lastAuthCheck: Long = 0

    fun shouldRetryAfterNetworkError(): Boolean {
        if (networkRetryCount < NETWORK_RETRY_MAX) {
            networkRetryCount += 1
            return true
        } else {
            return false
        }
    }

    fun newActionStarted() {
        networkRetryCount = 0
    }

    fun shouldRecheckAuthStatus(): Boolean {
        val elasped = (System.currentTimeMillis() - lastAuthCheck).milliseconds
        if (elasped > AUTH_RECOVERY_PERIOD) {
            lastAuthCheck = System.currentTimeMillis()
            return true
        } else {
            return false
        }
    }

    // For testing
    fun fastForward(amount: Duration) {
        lastAuthCheck -= amount.inWholeMilliseconds
    }
}

// Startup a top-level job that keeps processChannel() running until `close()` is called.
internal fun runActionProcessorManager(actionProcessor: FxaActionProcessor, coroutineContext: CoroutineContext) {
    CoroutineScope(coroutineContext).launch {
        while (true) {
            @Suppress("TooGenericExceptionCaught")
            try {
                actionProcessor.processChannel()
                // If processChannel returns, then it's time to quit
                break
            } catch (e: Exception) {
                Log.e(LOG_TAG, "Exception in processChannel", e)
            }
        }
    }
}

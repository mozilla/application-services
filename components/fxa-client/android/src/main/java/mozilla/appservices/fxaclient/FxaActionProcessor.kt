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
    private val channel = Channel<FxaAction>(100)
    internal val retryLogic = RetryLogic()
    internal var currentState = initialState

    @Volatile
    internal var simulateNetworkErrorFlag: Boolean = false

    // Queue a new action for processing
    fun queue(action: FxaAction) {
        // trySend allows this function to be non-suspend and will never fail since the channel size
        // is UNLIMITED
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
        val currentState = currentState
        // Match on the current state since many actions are only valid from a particular state
        try {
            FxaClientMetrics.operationCount.add()
            when (currentState) {
                is FxaAuthState.Connected -> when (action) {
                    is FxaAction.Disconnect -> handleDisconnect(action)
                    FxaAction.CheckAuthorization -> handleCheckAuthorization(currentState)
                    is FxaAction.InitializeDevice -> handleInitializeDevice(action)
                    is FxaAction.EnsureCapabilities -> handleEnsureCapabilities(action)
                    is FxaAction.SetDeviceName -> handleSetDeviceName(action)
                    is FxaAction.SetDevicePushSubscription -> handleSetDevicePushSubscription(action)
                    is FxaAction.SendSingleTab -> handleSendSingleTab(action)
                    else -> Log.e(LOG_TAG, "Invalid $action (state: $currentState)")
                }
                is FxaAuthState.Disconnected -> when (action) {
                    is FxaAction.BeginOAuthFlow -> handleBeginOAuthFlow(currentState, action)
                    is FxaAction.BeginPairingFlow -> handleBeginPairingFlow(currentState, action)
                    is FxaAction.CompleteOAuthFlow -> handleCompleteFlow(currentState, action)
                    is FxaAction.CancelOAuthFlow -> handleCancelFlow(currentState)
                    // If we see Disconnect or CheckAuthorization from the Disconnected state, just ignore it
                    FxaAction.CheckAuthorization -> Unit
                    is FxaAction.Disconnect -> Unit
                    else -> Log.e(LOG_TAG, "Invalid $action (state: $currentState)")
                }
            }
        } catch (e: FxaException) {
            FxaClientMetrics.errorCount["fxa_other"].add()
            throw e
        } catch (@Suppress("TooGenericExceptionCaught")e: Throwable) {
            FxaClientMetrics.errorCount["unexpected"].add()
            throw e
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

    private suspend fun changeState(newState: FxaAuthState, transition: FxaAuthStateTransition) {
        Log.d(LOG_TAG, "Changing state from $currentState to $newState")
        currentState = newState
        sendEvent(FxaEvent.AuthStateChanged(newState, transition))
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
                val currentState = currentState

                if (currentState !is FxaAuthState.Connected) {
                    throw e
                }

                if (retryLogic.shouldRecheckAuthStatus()) {
                    Log.d(LOG_TAG, "Auth error: re-checking")
                    handleCheckAuthorization(currentState)
                } else {
                    Log.d(LOG_TAG, "Auth error: disconnecting")
                    inner.disconnectFromAuthIssues()
                    persistState()
                    changeState(FxaAuthState.Disconnected(true), FxaAuthStateTransition.AUTH_CHECK_FAILED)
                }

                if (this.currentState is FxaAuthState.Connected) {
                    continue
                } else {
                    throw e
                }
            }
        }
    }

    private suspend fun handleBeginOAuthFlow(currentState: FxaAuthState.Disconnected, action: FxaAction.BeginOAuthFlow) {
        handleBeginEitherOAuthFlow(currentState, action.result) {
            inner.beginOauthFlow(action.scopes.toList(), action.entrypoint, MetricsParams(mapOf()))
        }
    }

    private suspend fun handleBeginPairingFlow(currentState: FxaAuthState.Disconnected, action: FxaAction.BeginPairingFlow) {
        handleBeginEitherOAuthFlow(currentState, action.result) {
            inner.beginPairingFlow(action.pairingUrl, action.scopes.toList(), action.entrypoint, MetricsParams(mapOf()))
        }
    }

    private suspend fun handleBeginEitherOAuthFlow(currentState: FxaAuthState.Disconnected, result: CompletableDeferred<String?>?, operation: () -> String) {
        try {
            val url = withRetry { operation() }
            persistState()
            changeState(currentState.copy(connecting = true), FxaAuthStateTransition.OAUTH_STARTED)
            sendEvent(FxaEvent.BeginOAuthFlow(url))
            result?.complete(url)
        } catch (e: FxaException) {
            FxaClientMetrics.errorCount["fxa_other"].add()
            Log.e(LOG_TAG, "Exception when handling BeginOAuthFlow", e)
            persistState()
            changeState(currentState.copy(connecting = false), FxaAuthStateTransition.OAUTH_FAILED_TO_BEGIN)
            result?.complete(null)
        }
    }

    private suspend fun handleCompleteFlow(currentState: FxaAuthState.Disconnected, action: FxaAction.CompleteOAuthFlow) {
        try {
            withRetry { inner.completeOauthFlow(action.code, action.state) }
            persistState()
            changeState(FxaAuthState.Connected(), FxaAuthStateTransition.OAUTH_COMPLETE)
        } catch (e: FxaException) {
            FxaClientMetrics.errorCount["fxa_other"].add()
            persistState()
            Log.e(LOG_TAG, "Exception when handling CompleteOAuthFlow", e)
            changeState(currentState, FxaAuthStateTransition.OAUTH_FAILED_TO_COMPLETE)
        }
    }

    private suspend fun handleCancelFlow(currentState: FxaAuthState.Disconnected) {
        // No need to call an inner method or persist the state, since the connecting flag is
        // handled soley in this layer
        changeState(currentState.copy(connecting = false), FxaAuthStateTransition.OAUTH_CANCELLED)
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

    private suspend fun handleDisconnect(action: FxaAction.Disconnect) {
        if (action.fromAuthIssues) {
            inner.disconnectFromAuthIssues()
            persistState()
            changeState(FxaAuthState.Disconnected(fromAuthIssues = true), FxaAuthStateTransition.AUTH_CHECK_FAILED)
        } else {
            inner.disconnect()
            persistState()
            changeState(FxaAuthState.Disconnected(), FxaAuthStateTransition.DISCONNECTED)
        }
    }

    private suspend fun handleCheckAuthorization(currentState: FxaAuthState.Connected) {
        changeState(currentState.copy(authCheckInProgress = true), FxaAuthStateTransition.AUTH_CHECK_STARTED)
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
            changeState(currentState.copy(authCheckInProgress = false), FxaAuthStateTransition.AUTH_CHECK_SUCCESS)
        } else {
            inner.disconnectFromAuthIssues()
            persistState()
            changeState(FxaAuthState.Disconnected(true), FxaAuthStateTransition.AUTH_CHECK_FAILED)
        }
    }
}

internal class RetryLogic {
    private var lastNetworkRetry: Long = 0
    private var lastAuthCheck: Long = 0

    fun shouldRetryAfterNetworkError(): Boolean {
        val elasped = (System.currentTimeMillis() - lastNetworkRetry).milliseconds
        if (elasped > 30.seconds) {
            lastNetworkRetry = System.currentTimeMillis()
            return true
        } else {
            return false
        }
    }

    fun shouldRecheckAuthStatus(): Boolean {
        val elasped = (System.currentTimeMillis() - lastAuthCheck).milliseconds
        if (elasped > 60.seconds) {
            lastAuthCheck = System.currentTimeMillis()
            return true
        } else {
            return false
        }
    }

    // For testing
    fun fastForward(amount: Duration) {
        lastNetworkRetry -= amount.inWholeMilliseconds
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

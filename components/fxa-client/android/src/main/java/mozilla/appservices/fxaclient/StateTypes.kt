/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.fxaclient

import kotlinx.coroutines.CompletableDeferred
import mozilla.appservices.sync15.DeviceType

/**
 * FxA Action.
 *
 * The application sends these to [FxaClient.queueAction].
 *
 * Note on [CompletableDeferred] result params:
 *
 * These exist for compatibility with the existing firefox-android codebase, but new code should use
 * the `FxaEventHandler` interface to listen for events.
 *
 * The reason is Deferred result processing is not guaranteed to happen in-order. For example:
 *   - Thread A sends a SetDeviceName action that fails.
 *   - Slightly later, thread B sends a SetDeviceName action that succeeds.
 *   - It's possible that the code to handle thread B's result will execute before the code for thread A.
 *   - This means that the user may see a connection warning when they shouldn't.
 *
 * This isn't ideal, but it seems like issues will be rare in practice.  The same issues could have
 * happened with the previous system, where the android-components code called sync methods, like
 * setDeviceName from async wrapper functions.
 */
sealed class FxaAction {
    /**
     * Begin an OAuth flow
     *
     * @param scopes OAuth scopes to request
     * @param entrypoint OAuth entrypoint
     * @param result If present, will be completed with the OAuth URL to navigate users too
     */
    data class BeginOAuthFlow(
        val scopes: Array<String>,
        val entrypoint: String,
        val result: CompletableDeferred<String?>? = null,
    ) : FxaAction()

    /**
     * Begin an OAuth flow using a paring code URL
     *
     * @param pairingUrl the url to initialize the paring flow with
     * @param scopes OAuth scopes to request
     * @param entrypoint OAuth entrypoint
     * @param result If present, will be completed with the OAuth URL to navigate users too
     */
    data class BeginPairingFlow(
        val pairingUrl: String,
        val scopes: Array<String>,
        val entrypoint: String,
        val result: CompletableDeferred<String?>? = null,
    ) : FxaAction()

    /**
     * Complete an OAuth flow, authenticating the current account.
     *
     * @param code query parameter from the redirect URL after completing the oauth flow
     * @param state query parameter from the redirect URL after completing the oauth flow
     */
    data class CompleteOAuthFlow(
        val code: String,
        val state: String,
    ) : FxaAction()

    /**
     * Cancel an OAuth flow
     */
    object CancelOAuthFlow : FxaAction()

    /**
     * Initialize device info on the server
     *
     * @param name Display name
     * @param deviceType Device type
     * @param supportedCapabilities Capabilities that the device supports
     * @param result If present, will be completed with true for success and false for failure
     */
    data class InitializeDevice(
        val name: String,
        val deviceType: DeviceType,
        val supportedCapabilities: List<DeviceCapability>,
        val result: CompletableDeferred<Boolean>? = null,
    ) : FxaAction()

    /**
     * Ensure capabilities are registered with the server
     *
     * @param supportedCapabilities Capabilities that the device supports
     * @param result If present, will be completed with true for success and false for failure
     */
    data class EnsureCapabilities(
        val supportedCapabilities: List<DeviceCapability>,
        val result: CompletableDeferred<Boolean>? = null,
    ) : FxaAction()

    /**
     * Update the display name (as shown in the FxA device manager, or the Send Tab target list)
     * for the current device.
     *
     * @param displayName The current device display name
     * @param result If present, will be completed with true for success and false for failure
     */
    data class SetDeviceName(
        val displayName: String,
        val result: CompletableDeferred<Boolean>? = null,
    ) : FxaAction()

    /**
     * Update the push subscription details for the current device.
     * This needs to be called every time a push subscription is modified or expires.
     *
     * @param endpoint Push callback URL
     * @param publicKey Public key used to encrypt push payloads
     * @param authKey Auth key used to encrypt push payloads
     * @param result If present, will be completed with true for success and false for failure
     */
    data class SetDevicePushSubscription(
        val endpoint: String,
        val publicKey: String,
        val authKey: String,
        val result: CompletableDeferred<Boolean>? = null,
    ) : FxaAction()

    /**
     * Send a single tab to another device identified by its device ID.
     *
     * @param targetDeviceId The target Device ID
     * @param title The document title of the tab being sent
     * @param url The url of the tab being sent
     * @param result If present, will be completed with true for success and false for failure
     */
    data class SendSingleTab(
        val targetDeviceId: String,
        val title: String,
        val url: String,
        val result: CompletableDeferred<Boolean>? = null,
    ) : FxaAction()

    /**
     * Disconnect from the FxA server and destroy our device record.
     *
     * @param fromAuthIssues: are we disconnecting because of auth issues?  Setting this flag
     * changes `FxaEvent.AuthStateChanged` so that the `fromAuthIssues` flag is will set and the
     * transition is `AUTH_CHECK_FAILED`
     */
    data class Disconnect(val fromAuthIssues: Boolean = false) : FxaAction()

    /**
     * Check the FxA authorization status.
     */
    object CheckAuthorization : FxaAction()
}

/**
 * Fxa event
 *
 * These are the results of FxaActions and are sent by the Fxa client to the application via the
 * FxaEventHandler interface.
 */
sealed class FxaEvent {
    /**
     * Called when the auth state changes.  Applications should use this to update their UI.
     */
    data class AuthStateChanged(
        val newState: FxaAuthState,
        val transition: FxaAuthStateTransition,
    ) : FxaEvent()

    /**
     * An action that updates the local device state completed successfully
     */
    data class DeviceOperationComplete(
        val operation: FxaDeviceOperation,
        val localDevice: LocalDevice,
    ) : FxaEvent()

    /**
     * An action that updates the local device state failed
     */
    data class DeviceOperationFailed(
        val operation: FxaDeviceOperation,
    ) : FxaEvent()

    /**
     * Called to begin an oauth flow.  The application must navigate the user to the URL to
     * start the process.
     */
    data class BeginOAuthFlow(val url: String) : FxaEvent()
}

/**
 * Kotlin authorization state class
 *
 * This is [FxaRustAuthState] with added data that Rust doesn't track yet.
 */
sealed class FxaAuthState {
    /**
     * Client has disconnected
     *
     * @property fromAuthIssues client was disconnected because of invalid auth tokens, for
     *   example because of a password reset on another device
     * @property connecting is there an OAuth flow in progress?
     */
    data class Disconnected(
        val fromAuthIssues: Boolean = false,
        val connecting: Boolean = false,
    ) : FxaAuthState()

    /**
     * Client is currently connected
     *
     * @property authCheckInProgress Client is checking the auth tokens and may disconnect soon
     */
    data class Connected(
        val authCheckInProgress: Boolean = false,
    ) : FxaAuthState()

    companion object {
        fun fromRust(authState: FxaRustAuthState): FxaAuthState {
            return when (authState) {
                is FxaRustAuthState.Connected -> FxaAuthState.Connected()
                is FxaRustAuthState.Disconnected -> {
                    FxaAuthState.Disconnected(authState.fromAuthIssues)
                }
            }
        }
    }
}

enum class FxaAuthStateTransition {
    OAUTH_STARTED,
    OAUTH_COMPLETE,
    OAUTH_CANCELLED,
    OAUTH_FAILED_TO_BEGIN,
    OAUTH_FAILED_TO_COMPLETE,
    DISCONNECTED,
    AUTH_CHECK_STARTED,
    AUTH_CHECK_FAILED,
    AUTH_CHECK_SUCCESS,
}

enum class FxaDeviceOperation {
    INITIALIZE_DEVICE,
    ENSURE_CAPABILITIES,
    SET_DEVICE_NAME,
    SET_DEVICE_PUSH_SUBSCRIPTION,
}

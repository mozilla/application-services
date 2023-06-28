/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.components.service.fxa

import mozilla.appservices.fxaclient.AccessTokenInfo
import mozilla.appservices.fxaclient.Device
import mozilla.appservices.fxaclient.DeviceCapability
import mozilla.appservices.fxaclient.DevicePushSubscription
import mozilla.appservices.fxaclient.FxaConfig
import mozilla.appservices.fxaclient.FirefoxAccount as RustFirefoxAccount
import mozilla.appservices.fxaclient.IncomingDeviceCommand
import mozilla.appservices.fxaclient.Profile
import mozilla.appservices.sync15.DeviceType
import kotlin.coroutines.CoroutineContext

/**
 * Top-level class to manage a FxA account
 *
 * This class contains the top-level functionality like fetching tokens and listening for account
 * events.  See also:
 *   - StateManager: manages the account state machine.  Defined in `State.kt` and available from
 *     the `stateManager()` method.
 *   - DeviceManager: manages the list of connected devices to an FxA account.  Defined in
 *     `Devices.kt` and available from the `deviceManager()` method.
 */
abstract class FirefoxAccount(
    val config: FxaConfig,
    val deviceConfig: DeviceConfig,
    val storageHandler: StorageHandler,
    val applicationScopes: List<Scope> = listOf(),
) {
    /**
     * Tries to fetch an access token for the given scope.
     *
     * @param singleScope Single OAuth scope (no spaces) for which the client wants access
     * @return [AccessTokenInfo] that stores the token, along with its scope, key and
     *                           expiration timestamp (in seconds) since epoch when complete
     * @throws FxaException.Authentication if the account is not connected
     */
    abstract suspend fun getAccessToken(singleScope: String): AccessTokenInfo

    /**
     * Fetches the profile object for the current client either from the existing cached state
     * or from the server (requires the client to have access to the profile scope).
     *
     * @param ignoreCache Fetch the profile information directly from the server
     * @return Profile representing the user's basic profile info
     * @throws FxaException.Authentication if the account is not connected
     */
    abstract suspend fun getProfile(ignoreCache: Boolean = false): Profile

    /**
     * Fetches the token server endpoint, for authentication using the SAML bearer flow.
     *
     * @return Token server endpoint URL string
     * @throws FxaException.Authentication if the account is not connected
     */
    abstract suspend fun getTokenServerEndpointURL(): String

    /**
     * Get the pairing URL to navigate to on the Authority side (typically a computer).
     *
     * @return The URL to show the pairing user
     */
    abstract fun getPairingAuthorityURL(): String

    /**
     * Returns current FxA Device ID for an authenticated account.
     *
     * @return Current device's FxA ID
     * @throws FxaException.Authentication if the account is not connected
     */
    abstract fun getCurrentDeviceId(): String

    /**
     * Returns session token for an authenticated account.
     *
     * @return Current account's session token
     * @throws FxaException.Authentication if the account is not connected
     */
    abstract fun getSessionToken(): String

    /**
     * Indicates if account needs to be re-authenticated via [beginAuthentication].
     * Most common reason for an account to need re-authentication is a password change.
     *
     * TODO this may return a false-positive, if we're currently going through a recovery flow.
     * Prefer to be notified of auth problems via [AccountObserver], which is reliable.
     *
     * @return A boolean flag indicating if account needs to be re-authenticated.
     */
    abstract fun accountNeedsReauth(): Boolean

    /**
     * This method should be called when a request made with an OAuth token failed with an
     * authentication error. It will re-build cached state and perform a connectivity check.
     *
     * In time, fxalib will grow a similar method, at which point we'll just relay to it.
     * See https://github.com/mozilla/application-services/issues/1263
     *
     * @param singleScope An oauth scope for which to check authorization state.
     * @return An optional [Boolean] flag indicating if we're connected, or need to go through
     * re-authentication. A null result means we were not able to determine state at this time.
     */
    abstract suspend fun checkAuthorizationStatus(singleScope: String): Boolean?

    /**
     * Register for account events
     */
    abstract suspend fun registerAccountEventHandler(handler: AccountEventHandler)

    abstract fun stateManager(): StateManager

    abstract fun deviceManager(): DeviceManager
}

interface AccountEventHandler {
    fun onAccountEvent(event: AccountEvent)
}

sealed class AccountEvent  {
    /** The account's profile was updated */
    object ProfileUpdated : AccountEvent()

    /** The authentication state of the account changed - eg, the password changed */
    data class AccountStateChanged(val state: AccountState) : AccountEvent()

    /** The account itself was destroyed */
    object AccountDestroyed : AccountEvent()

    /** An incoming command from another device */
    data class DeviceCommandIncoming(val command: IncomingDeviceCommand) : AccountEvent()

    /** Another device connected to the account */
    data class DevicesChanged(val deviceList: DeviceList) : AccountEvent()
}

interface StorageHandler {
    /**
     * Called when the FirefoxAccount state has changed and should be saved
     *
     * The `state` that was last passed to this method is what should be passed back in
     * `StateManager.initialize()`
     */
    fun saveState(state: String)
}

/** Scopes that can be requested from the FxA Server */
enum class Scope {
    // Fetch the user's profile.
    Profile,
    // Obtain sync keys for the user
    Sync,
    // Obtain a sessionToken, which gives full access to the account.
    Session,
}

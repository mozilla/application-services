/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.components.service.fxa

/**
 * Manages state transitions for a FirefoxAccount
 *
 * FirefoxAccount uses a state-machine to track it's current status.  This class contains the
 * methods for state transitions.  Example usage:
 *
 *  - If the user isn't logged in, the FirefoxAccount is in the `Disconnected` state.
 *  - When the user clicks the connect button, the application opens a view for the oauth flow and
 *    calls the `beginOAuthFlow()`.
 *  - If the user goes through all the OAuth steps, the application closes its view and calls
 *   `completeOAuthFlow()`.
 *  - If the user cancels the flow, the application closes its view and calls `cancelOauthFlow()`.
 */
abstract class StateManager {
    /**
     * Get the current state
     */
    abstract suspend fun getState(): AccountState

    /**
     * Initialize the FirefoxAccount
     *
     * When a `FirefoxAccount` is first constructed, it's in the `Uninitialized` state waiting for this
     * call.  This allows the application to construct a `FirefoxAccount` immediately, but wait to
     * load the saved state from disk until later on in the startup process.
     *
     * @param savedState: the saved state from the last StorageHandler.saveState call, if available.
     * @throws FxaException.InvalidState: the account was not in the `Uninitialized` state
     */
    abstract suspend fun initialize(savedState: String?)

    /**
     * Initiate an OAuth flow and get the URL send the user to
     *
     * If there is already an auth flow in progress, this will cancel it and start a new one.
     * Attempts to pass the state/code from the previous flow to `completeOAuthFlow()` will cause
     * `FxaException.WrongAuthFlow` to be thrown.
     *
     * @param scopes List of OAuth scopes for which the client wants access
     * @param entryPoint The UI entryPoint used to start this flow. An arbitrary
     * string which is recorded in telemetry by the server to help analyze the
     * most effective touchpoints
     * @return url to navigate the user to
     * @throws FxaException.InvalidState: the account was not in the `Disconnected` state
     */
    abstract suspend fun beginOAuthFlow(
        entryPoint: String,
    ): String

    /**
     * Initiate an OAuth pairing flow and get the URL send the user to
     *
     * If there is already an auth flow in progress, this will cancel it and start a new one.
     * Attempts to pass the state/code from the previous flow to `completeOAuthFlow()` will cause
     * `FxaException.WrongAuthFlow` to be thrown.
     *
     * @param pairingUrl URL string for pairing
     * @param scopes List of OAuth scopes for which the client wants access
     * @param entryPoint The UI entryPoint used to start this flow. An arbitrary
     * string which is recorded in telemetry by the server to help analyze the
     * most effective touchpoints
     * @return url to navigate the user to
     * @throws FxaException.InvalidState: the account was not in the `Disconnected` state
     */
    abstract suspend fun beginPairingFlow(
        pairingUrl: String,
        entryPoint: String,
    ): String

    /**
     * Authenticates the current account using the [code] and [state] parameters obtained via the
     * OAuth flow initiated by [beginOAuthFlow].
     *
     * @param code OAuth code string
     * @param state state token string
     * @throws FxaException.InvalidState: the account was not in the `Disconnected` state
     * @throws FxaException.WrongAuthFlow: the code/state is not valid for the current auth flow
     */
    abstract suspend fun completeOAuthFlow(code: String, state: String)

    /**
     * Cancels any current auth flow
     *
     * @throws FxaException.InvalidState: the account was not in the `Disconnected` state
     */
    abstract suspend fun cancelOauthFlow()

    /**
     * Disconnect the account.
     *
     * Use this when device record is no longer relevant, e.g. while logging out. On success, other
     * devices will no longer see the current device in their device lists.
     *
     * @throws FxaException.InvalidState: the account was not in the `Connected` state
     */
    abstract suspend fun disconnect()
}

sealed class AccountState  {
    /** The account is waiting for the `StateManager.initialize()` call */
    object Uninitialized : AccountState()

    /**
     * The account is disconnected from the FxA server.  The user needs to perform an oauth flow to log in.
     */
    data class Disconnected(
        /**
         * the user was disconnected because of auth issues. Applications can use this to present a
         * `Reconnect` action rather than "log in"
         */
        val fromAuthIssues: Boolean,
        /** Is there an oauth flow in progress? */
        val connecting: Boolean
    ) : AccountState()

    /**
     * The account is connected from the FxA server.
     */
    object Connected : AccountState()
}

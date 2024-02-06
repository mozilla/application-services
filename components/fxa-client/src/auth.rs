/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Signing in and out
//!
//! These are methods for managing the signed-in state, such as authenticating via
//! an OAuth flow or disconnecting from the user's account.
//!
//! The Firefox Accounts system supports two methods for connecting an application
//! to a user's account:
//!
//!    - A traditional OAuth flow, where the user is directed to a webpage to enter
//!      their account credentials and then redirected back to the application.
//!      This is exposed by the [`begin_oauth_flow`](FirefoxAccount::begin_oauth_flow)
//!      method.
//!
//!    - A device pairing flow, where the user scans a QRCode presented by another
//!      app that is already connected to the account, which then directs them to
//!      a webpage for a simplified signing flow. This is exposed by the
//!      [`begin_pairing_flow`](FirefoxAccount::begin_pairing_flow) method.
//!
//! Technical details of the pairing flow can be found in the [Firefox Accounts
//! documentation hub](https://mozilla.github.io/ecosystem-platform/docs/features/firefox-accounts/pairing).

use crate::{ApiResult, DeviceConfig, Error, FirefoxAccount};
use error_support::handle_error;

impl FirefoxAccount {
    /// Get the current state
    pub fn get_state(&self) -> FxaState {
        self.internal.lock().get_state()
    }

    /// Process an event (login, logout, etc).
    ///
    /// On success, returns the new state.
    /// On error, the state will remain the same.
    #[handle_error(Error)]
    pub fn process_event(&self, event: FxaEvent) -> ApiResult<FxaState> {
        self.internal.lock().process_event(event)
    }

    /// Get the high-level authentication state of the client
    ///
    /// TODO: remove this and the FxaRustAuthState type from the public API
    /// https://bugzilla.mozilla.org/show_bug.cgi?id=1868614
    pub fn get_auth_state(&self) -> FxaRustAuthState {
        self.internal.lock().get_auth_state()
    }

    /// Sets the user data for a user agent
    /// **Important**: This should only be used on user agents such as Firefox
    /// that require the user's session token
    pub fn set_user_data(&self, user_data: UserData) {
        self.internal.lock().set_user_data(user_data)
    }

    /// Initiate a web-based OAuth sign-in flow.
    ///
    /// This method initializes some internal state and then returns a URL at which the
    /// user may perform a web-based authorization flow to connect the application to
    /// their account. The application should direct the user to the provided URL.
    ///
    /// When the resulting OAuth flow redirects back to the configured `redirect_uri`,
    /// the query parameters should be extracting from the URL and passed to the
    /// [`complete_oauth_flow`](FirefoxAccount::complete_oauth_flow) method to finalize
    /// the signin.
    ///
    /// # Arguments
    ///
    ///   - `scopes` - list of OAuth scopes to request.
    ///       - The requested scopes will determine what account-related data
    ///         the application is able to access.
    ///   - `entrypoint` - metrics identifier for UX entrypoint.
    ///       - This parameter is used for metrics purposes, to identify the
    ///         UX entrypoint from which the user triggered the signin request.
    ///         For example, the application toolbar, on the onboarding flow.
    ///   - `metrics` - optionally, additional metrics tracking parameters.
    ///       - These will be included as query parameters in the resulting URL.
    #[handle_error(Error)]
    pub fn begin_oauth_flow<T: AsRef<str>>(
        &self,
        // Allow both &[String] and &[&str] since UniFFI can't represent `&[&str]` yet,
        scopes: &[T],
        entrypoint: &str,
    ) -> ApiResult<String> {
        let scopes = scopes.iter().map(T::as_ref).collect::<Vec<_>>();
        self.internal.lock().begin_oauth_flow(&scopes, entrypoint)
    }

    /// Get the URL at which to begin a device-pairing signin flow.
    ///
    /// If the user wants to sign in using device pairing, call this method and then
    /// direct them to visit the resulting URL on an already-signed-in device. Doing
    /// so will trigger the other device to show a QR code to be scanned, and the result
    /// from said QR code can be passed to [`begin_pairing_flow`](FirefoxAccount::begin_pairing_flow).
    #[handle_error(Error)]
    pub fn get_pairing_authority_url(&self) -> ApiResult<String> {
        self.internal.lock().get_pairing_authority_url()
    }

    /// Initiate a device-pairing sign-in flow.
    ///
    /// Once the user has scanned a pairing QR code, pass the scanned value to this
    /// method. It will return a URL to which the application should redirect the user
    /// in order to continue the sign-in flow.
    ///
    /// When the resulting flow redirects back to the configured `redirect_uri`,
    /// the resulting OAuth parameters should be extracting from the URL and passed
    /// to [`complete_oauth_flow`](FirefoxAccount::complete_oauth_flow) to finalize
    /// the signin.
    ///
    /// # Arguments
    ///
    ///   - `pairing_url` - the URL scanned from a QR code on another device.
    ///   - `scopes` - list of OAuth scopes to request.
    ///       - The requested scopes will determine what account-related data
    ///         the application is able to access.
    ///   - `entrypoint` - metrics identifier for UX entrypoint.
    ///       - This parameter is used for metrics purposes, to identify the
    ///         UX entrypoint from which the user triggered the signin request.
    ///         For example, the application toolbar, on the onboarding flow.
    ///   - `metrics` - optionally, additional metrics tracking parameters.
    ///       - These will be included as query parameters in the resulting URL.
    #[handle_error(Error)]
    pub fn begin_pairing_flow(
        &self,
        pairing_url: &str,
        scopes: &[String],
        entrypoint: &str,
    ) -> ApiResult<String> {
        // UniFFI can't represent `&[&str]` yet, so convert it internally here.
        let scopes = scopes.iter().map(String::as_str).collect::<Vec<_>>();
        self.internal
            .lock()
            .begin_pairing_flow(pairing_url, &scopes, entrypoint)
    }

    /// Complete an OAuth flow.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// At the conclusion of an OAuth flow, the user will be redirect to the
    /// application's registered `redirect_uri`. It should extract the `code`
    /// and `state` parameters from the resulting URL and pass them to this
    /// method in order to complete the sign-in.
    ///
    /// # Arguments
    ///
    ///   - `code` - the OAuth authorization code obtained from the redirect URI.
    ///   - `state` - the OAuth state parameter obtained from the redirect URI.
    #[handle_error(Error)]
    pub fn complete_oauth_flow(&self, code: &str, state: &str) -> ApiResult<()> {
        self.internal.lock().complete_oauth_flow(code, state)
    }

    /// Check authorization status for this application.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications may call this method to check with the FxA server about the status
    /// of their authentication tokens. It returns an [`AuthorizationInfo`] struct
    /// with details about whether the tokens are still active.
    #[handle_error(Error)]
    pub fn check_authorization_status(&self) -> ApiResult<AuthorizationInfo> {
        Ok(self.internal.lock().check_authorization_status()?.into())
    }

    /// Disconnect from the user's account.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// This method destroys any tokens held by the client, effectively disconnecting
    /// from the user's account. Applications should call this when the user opts to
    /// sign out.
    ///
    /// The persisted account state after calling this method will contain only the
    /// user's last-seen profile information, if any. This may be useful in helping
    /// the user to reconnect to their account. If reconnecting to the same account
    /// is not desired then the application should discard the persisted account state.
    pub fn disconnect(&self) {
        self.internal.lock().disconnect()
    }

    /// Update the state based on authentication issues.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Call this if you know there's an authentication / authorization issue that requires the
    /// user to re-authenticated.  It transitions the user to the [FxaRustAuthState.AuthIssues] state.
    pub fn on_auth_issues(&self) {
        self.internal.lock().on_auth_issues()
    }

    /// Used by the application to test auth token issues
    pub fn simulate_temporary_auth_token_issue(&self) {
        self.internal.lock().simulate_temporary_auth_token_issue()
    }

    /// Used by the application to test auth token issues
    pub fn simulate_permanent_auth_token_issue(&self) {
        self.internal.lock().simulate_permanent_auth_token_issue()
    }
}

/// Information about the authorization state of the application.
///
/// This struct represents metadata about whether the application is currently
/// connected to the user's account.
pub struct AuthorizationInfo {
    pub active: bool,
}

/// High-level view of the authorization state
///
/// This is named `FxaRustAuthState` because it doesn't track all the states we want yet and needs
/// help from the wrapper code.  The wrapper code defines the actual `FxaAuthState` type based on
/// this, adding the extra data.
///
/// In the long-term, we should track that data in Rust, remove the wrapper, and rename this to
/// `FxaAuthState`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FxaRustAuthState {
    Disconnected,
    Connected,
    AuthIssues,
}

/// Fxa state
///
/// These are the states of [crate::FxaStateMachine] that consumers observe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FxaState {
    /// The state machine needs to be initialized via [Event::Initialize].
    Uninitialized,
    /// User has not connected to FxA or has logged out
    Disconnected,
    /// User is currently performing an OAuth flow
    Authenticating { oauth_url: String },
    /// User is currently connected to FxA
    Connected,
    /// User was connected to FxA, but we observed issues with the auth tokens.
    /// The user needs to reauthenticate before the account can be used.
    AuthIssues,
}

/// Fxa event
///
/// These are the events that consumers send to [crate::FxaStateMachine::process_event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FxaEvent {
    /// Initialize the state machine.  This must be the first event sent.
    Initialize { device_config: DeviceConfig },
    /// Begin an oauth flow
    ///
    /// If successful, the state machine will transition the [FxaState::Authenticating].  The next
    /// step is to navigate the user to the `oauth_url` and let them sign and authorize the client.
    BeginOAuthFlow {
        scopes: Vec<String>,
        entrypoint: String,
    },
    /// Begin an oauth flow using a URL from a pairing code
    ///
    /// If successful, the state machine will transition the [FxaState::Authenticating].  The next
    /// step is to navigate the user to the `oauth_url` and let them sign and authorize the client.
    BeginPairingFlow {
        pairing_url: String,
        scopes: Vec<String>,
        entrypoint: String,
    },
    /// Complete an OAuth flow.
    ///
    /// Send this event after the user has navigated through the OAuth flow and has reached the
    /// redirect URI.  Extract `code` and `state` from the query parameters or web channel.  If
    /// successful the state machine will transition to [FxaState::Connected].
    CompleteOAuthFlow { code: String, state: String },
    /// Cancel an OAuth flow.
    ///
    /// Use this to cancel an in-progress OAuth, returning to [FxaState::Disconnected] so the
    /// process can begin again.
    CancelOAuthFlow,
    /// Check the authorization status for a connected account.
    ///
    /// Send this when issues are detected with the auth tokens for a connected account.  It will
    /// double check for authentication issues with the account.  If it detects them, the state
    /// machine will transition to [FxaState::AuthIssues].  From there you can start an OAuth flow
    /// again to re-connect the user.
    CheckAuthorizationStatus,
    /// Disconnect the user
    ///
    /// Send this when the user is asking to be logged out.  The state machine will transition to
    /// [FxaState::Disconnected].
    Disconnect,
    /// Force a call to [FirefoxAccount::get_profile]
    ///
    /// This is used for testing the auth/network retry code, since it hits the network and
    /// requires and auth token.
    CallGetProfile,
}

/// User data provided by the web content, meant to be consumed by user agents
#[derive(Debug, Clone)]
pub struct UserData {
    pub(crate) session_token: String,
    pub(crate) uid: String,
    pub(crate) email: String,
    pub(crate) verified: bool,
}

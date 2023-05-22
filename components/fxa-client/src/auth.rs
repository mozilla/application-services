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

use crate::{ApiResult, Error, FirefoxAccount};
use error_support::handle_error;
use std::collections::HashMap;

impl FirefoxAccount {
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
    pub fn begin_oauth_flow(
        &self,
        scopes: &[String],
        entrypoint: &str,
        metrics: Option<MetricsParams>,
    ) -> ApiResult<String> {
        // UniFFI can't represent `&[&str]` yet, so convert it internally here.
        let scopes = scopes.iter().map(String::as_str).collect::<Vec<_>>();
        self.internal
            .lock()
            .unwrap()
            .begin_oauth_flow(&scopes, entrypoint, metrics)
    }

    /// Get the URL at which to begin a device-pairing signin flow.
    ///
    /// If the user wants to sign in using device pairing, call this method and then
    /// direct them to visit the resulting URL on an already-signed-in device. Doing
    /// so will trigger the other device to show a QR code to be scanned, and the result
    /// from said QR code can be passed to [`begin_pairing_flow`](FirefoxAccount::begin_pairing_flow).
    #[handle_error(Error)]
    pub fn get_pairing_authority_url(&self) -> ApiResult<String> {
        self.internal.lock().unwrap().get_pairing_authority_url()
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
        metrics: Option<MetricsParams>,
    ) -> ApiResult<String> {
        // UniFFI can't represent `&[&str]` yet, so convert it internally here.
        let scopes = scopes.iter().map(String::as_str).collect::<Vec<_>>();
        self.internal
            .lock()
            .unwrap()
            .begin_pairing_flow(pairing_url, &scopes, entrypoint, metrics)
    }

    /// Complete an OAuth flow.
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
        self.internal
            .lock()
            .unwrap()
            .complete_oauth_flow(code, state)
    }

    /// Check authorization status for this application.
    ///
    /// Applications may call this method to check with the FxA server about the status
    /// of their authentication tokens. It returns an [`AuthorizationInfo`] struct
    /// with details about whether the tokens are still active.
    #[handle_error(Error)]
    pub fn check_authorization_status(&self) -> ApiResult<AuthorizationInfo> {
        Ok(self
            .internal
            .lock()
            .unwrap()
            .check_authorization_status()?
            .into())
    }

    /// Disconnect from the user's account.
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
        self.internal.lock().unwrap().disconnect()
    }
}

/// Information about the authorization state of the application.
///
/// This struct represents metadata about whether the application is currently
/// connected to the user's account.
pub struct AuthorizationInfo {
    pub active: bool,
}

/// Additional metrics tracking parameters to include in an OAuth request.
pub struct MetricsParams {
    pub parameters: HashMap<String, String>,
}

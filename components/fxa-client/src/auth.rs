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

use crate::{ApiResult, Error, FirefoxAccount, FxaError};
use error_support::handle_error;
use std::collections::HashMap;

impl FirefoxAccount {
    /// Initiate an web-based OAuth sign-in flow.
    ///
    /// - This method calls `perform_flow` on the `OAuthHandler` instance passed to the constructor.
    /// - While `perform_flow()` is running, the `FirefoxAccount` instance will be in the
    ///   [FxaState::Authenticated] state (see the FxaState documentation for details)
    /// - When `perform_flow()` returns the state will be changed to `Authenticated` on success and
    ///   `Disconnected` on failure.
    ///
    /// Note: `begin_oauth_flow()` currently blocks while the OAuth operation is in-progress.
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
    ) -> ApiResult<()> {
        unimplemented!()
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
    /// This method follows the same general flow as `begin_oauth_flow()`, but uses a pairing URL
    /// taken from the scanned value of a QR code.
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
    ) -> ApiResult<()> {
        unimplemented!()
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

/// OAuth handler.  These are defined in the foreign code
pub trait OAuthHandler {
    /// Perform an OAuth flow at a URL
    ///
    /// When the resulting OAuth flow redirects back to the configured `redirect_uri`,
    /// the query parameters should be extracting from the URL and returned.
    ///
    /// Warning: the `FirefoxAccount` instance will be in the `Authorizing` state while this
    /// method is running.  Consumers must make sure the method eventually returns or the
    /// `FirefoxAccount` instance will be stuck.  Return `FxaError::Cancelled` for abandoned OAuth
    /// sessions.
    fn perform_flow(&self, url: String) -> Result<OAuthResult, FxaError>;

    /// Cancel the current OAuth flow
    fn cancel(&self);
}

// Result of an Oauth flow
//
// Normally, the field values are extracted from the URL query parameters when the browser reaches
// the redirect_uri.
pub struct OAuthResult {
    pub code: String,
    pub state: String,
}

/// Additional metrics tracking parameters to include in an OAuth request.
pub struct MetricsParams {
    pub parameters: HashMap<String, String>,
}

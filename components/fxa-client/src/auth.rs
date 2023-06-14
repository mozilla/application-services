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
//!      This is exposed by the [`connect_with_oauth`](FirefoxAccount::connect_with_oauth)
//!      method.
//!
//!    - A device pairing flow, where the user scans a QRCode presented by another
//!      app that is already connected to the account, which then directs them to
//!      a webpage for a simplified signing flow. This is exposed by the
//!      [`connect_with_pairing`](FirefoxAccount::connect_with_pairing) method.
//!
//! Technical details of the pairing flow can be found in the [Firefox Accounts
//! documentation hub](https://mozilla.github.io/ecosystem-platform/docs/features/firefox-accounts/pairing).

use crate::{ApiResult, CallbackResult, Error, FirefoxAccount};
use error_support::handle_error;
use std::collections::HashMap;

impl FirefoxAccount {
    /// Connect to FxA using a web-based OAuth sign-in flow.
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
    pub fn connect_with_oauth<T: AsRef<str>>(
        &self,
        // Allow both &[String] and &[&str] since UniFFI can't represent `&[&str]` yet,
        scopes: &[T],
        entrypoint: &str,
        metrics: Option<MetricsParams>,
    ) -> ApiResult<()> {
        let scopes = scopes.iter().map(T::as_ref).collect::<Vec<_>>();

        // Lock self.internal to determine the oauth URL
        let mut internal = self.internal.lock();
        let (url, flow) = internal.begin_oauth_flow(&scopes, entrypoint, metrics)?;

        // Release the lock while blocking on the OAuth handler
        drop(internal);
        let result = self.oauth_handler.perform_flow(url)?;

        // Take the lock again to complete the flow
        let mut internal = self.internal.lock();
        internal.complete_oauth_flow(flow, result)?;
        Ok(())
    }

    /// Get the URL at which to begin a device-pairing signin flow.
    ///
    /// If the user wants to sign in using device pairing, call this method and then
    /// direct them to visit the resulting URL on an already-signed-in device. Doing
    /// so will trigger the other device to show a QR code to be scanned, and the result
    /// from said QR code can be passed to
    /// [`connect_with_pairing`](FirefoxAccount::connect_with_pairing).
    #[handle_error(Error)]
    pub fn get_pairing_authority_url(&self) -> ApiResult<String> {
        self.internal.lock().get_pairing_authority_url()
    }

    /// Connect to FxA using a device-pairing sign-in flow.
    ///
    /// Once the user has scanned a pairing QR code, pass the scanned value to this
    /// method. It will return a URL to which the application should redirect the user
    /// in order to continue the sign-in flow.
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
    pub fn connect_with_pairing<T: AsRef<str>>(
        &self,
        pairing_url: &str,
        // Allow both &[String] and &[&str] since UniFFI can't represent `&[&str]` yet,
        scopes: &[T],
        entrypoint: &str,
        metrics: Option<MetricsParams>,
    ) -> ApiResult<()> {
        let scopes = scopes.iter().map(T::as_ref).collect::<Vec<_>>();

        // Lock self.internal to determine the oauth URL
        let mut internal = self.internal.lock();
        let (url, flow) = internal.begin_pairing_flow(pairing_url, &scopes, entrypoint, metrics)?;

        // Release the lock while blocking on the OAuth handler
        drop(internal);
        let result = self.oauth_handler.perform_flow(url)?;

        // Take the lock again to complete the flow
        let mut internal = self.internal.lock();
        internal.complete_oauth_flow(flow, result)?;
        Ok(())
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
}

/// Information about the authorization state of the application.
///
/// This struct represents metadata about whether the application is currently
/// connected to the user's account.
pub struct AuthorizationInfo {
    pub active: bool,
}

/// OAuth handler.  These are defined in the foreign code
pub trait OAuthHandler: Send + Sync {
    /// Perform an OAuth flow at a URL
    ///
    /// When the resulting OAuth flow redirects back to the configured `redirect_uri`,
    /// the query parameters should be extracting from the URL and returned.
    ///
    /// Warning: the `FirefoxAccount` instance will be in the `Authorizing` state while this
    /// method is running.  Consumers must make sure the method eventually returns or the
    /// `FirefoxAccount` instance will be stuck.  Return `FxaError::Cancelled` for abandoned OAuth
    /// sessions.
    fn perform_flow(&self, url: String) -> CallbackResult<OAuthResult>;
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

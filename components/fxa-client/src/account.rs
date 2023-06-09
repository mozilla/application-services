/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Account Management URLs
//!
//! Signed-in applications should not attempt to perform an account-level management
//! (such as changing profile data or managing devices) using native UI. Instead, they
//! should offer the user the opportunity to visit their account management pages on the
//! web.
//!
//! The methods in this section provide URLs at which the user can perform various
//! account-management activities.

use crate::{ApiResult, Error, FirefoxAccount};
use error_support::handle_error;

impl FirefoxAccount {
    /// Get the token server URL
    ///
    /// The token server URL can be used to get the URL and access token for the user's sync data.
    ///
    /// **💾 This method alters the persisted account state.**
    #[handle_error(Error)]
    pub fn get_token_server_endpoint_url(&self) -> ApiResult<String> {
        self.internal.lock().get_token_server_endpoint_url()
    }

    /// Get a URL which shows a "successfully connected!" message.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications can use this method after a successful signin, to redirect the
    /// user to a success message displayed in web content rather than having to
    /// implement their own native success UI.
    #[handle_error(Error)]
    pub fn get_connection_success_url(&self) -> ApiResult<String> {
        self.internal.lock().get_connection_success_url()
    }

    /// Get a URL at which the user can manage their account and profile data.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications should link the user out to this URL from an appropriate place
    /// in their signed-in settings UI.
    ///
    /// # Arguments
    ///
    ///   - `entrypoint` - metrics identifier for UX entrypoint.
    ///       - This parameter is used for metrics purposes, to identify the
    ///         UX entrypoint from which the user followed the link.
    #[handle_error(Error)]
    pub fn get_manage_account_url(&self, entrypoint: &str) -> ApiResult<String> {
        self.internal.lock().get_manage_account_url(entrypoint)
    }

    /// Get a URL at which the user can manage the devices connected to their account.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications should link the user out to this URL from an appropriate place
    /// in their signed-in settings UI. For example, "Manage your devices..." may be
    /// a useful link to place somewhere near the device list in the send-tab UI.
    ///
    /// # Arguments
    ///
    ///   - `entrypoint` - metrics identifier for UX entrypoint.
    ///       - This parameter is used for metrics purposes, to identify the
    ///         UX entrypoint from which the user followed the link.
    #[handle_error(Error)]
    pub fn get_manage_devices_url(&self, entrypoint: &str) -> ApiResult<String> {
        self.internal.lock().get_manage_devices_url(entrypoint)
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # User Profile info
//!
//! These methods can be used to find out information about the connected user.

use crate::{ApiResult, Error, FirefoxAccount};
use error_support::handle_error;

impl FirefoxAccount {
    /// Get profile information for the signed-in user, if any.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// This method fetches a [`Profile`] struct with information about the currently-signed-in
    /// user, either by using locally-cached profile information or by fetching fresh data from
    /// the server.
    ///
    /// # Arguments
    ///
    ///    - `ignore_cache` - if true, always hit the server for fresh profile information.
    ///
    /// # Notes
    ///
    ///    - Profile information is only available to applications that have been
    ///      granted the `profile` scope.
    ///    - There is currently no API for fetching cached profile information without
    ///      potentially hitting the server.
    ///    - If there is no signed-in user, this method will throw an
    ///      [`Authentication`](FxaError::Authentication) error.
    #[handle_error(Error)]
    pub fn get_profile(&self, ignore_cache: bool) -> ApiResult<Profile> {
        Ok(self.internal.lock().get_profile(ignore_cache)?.into())
    }
}

/// Information about the user that controls a Firefox Account.
///
/// This struct represents details about the user themselves, and would typically be
/// used to customize account-related UI in the browser so that it is personalize
/// for the current user.
pub struct Profile {
    /// The user's account uid
    ///
    /// This is an opaque immutable unique identifier for their account.
    pub uid: String,
    /// The user's current primary email address.
    ///
    /// Note that unlike the `uid` field, the email address may change over time.
    pub email: String,
    /// The user's preferred textual display name.
    pub display_name: Option<String>,
    /// The URL of a profile picture representing the user.
    ///
    /// All accounts have a corresponding profile picture. If the user has not
    /// provided one then a default image is used.
    pub avatar: String,
    /// Whether the `avatar` URL represents the default avatar image.
    pub is_default_avatar: bool,
}

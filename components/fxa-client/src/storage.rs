/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # State management
//!
//! These are methods for managing the signed-in state of the application,
//! either by restoring a previously-saved state via [`FirefoxAccount::from_json`]
//! or by starting afresh with [`FirefoxAccount::new`].
//!
//! The application must persist the signed-in state after calling any methods
//! that may alter it. Such methods are marked in the documentation as follows:
//!
//! **💾 This method alters the persisted account state.**
//!
//! After calling any such method, use [`FirefoxAccount::to_json`] to serialize
//! the modified account state and persist the resulting string in application
//! settings.

use crate::{internal, ApiResult, Error, FirefoxAccount};
use error_support::handle_error;
use parking_lot::Mutex;

impl FirefoxAccount {
    /// Restore a [`FirefoxAccount`] instance from serialized state.
    ///
    /// Given a JSON string previously obtained from [`FirefoxAccount::to_json`], this
    /// method will deserialize it and return a live [`FirefoxAccount`] instance.
    ///
    /// **⚠️ Warning:** since the serialized state contains access tokens, you should
    /// not call `from_json` multiple times on the same data. This would result
    /// in multiple live objects sharing the same access tokens and is likely to
    /// produce unexpected behaviour.
    #[handle_error(Error)]
    pub fn from_json(data: &str) -> ApiResult<FirefoxAccount> {
        Ok(FirefoxAccount {
            internal: Mutex::new(internal::FirefoxAccount::from_json(data)?),
        })
    }

    /// Save current state to a JSON string.
    ///
    /// This method serializes the current account state into a JSON string, which
    /// the application can use to persist the user's signed-in state across restarts.
    /// The application should call this method and update its persisted state after
    /// any potentially-state-changing operation.
    ///
    /// **⚠️ Warning:** the serialized state may contain encryption keys and access
    /// tokens that let anyone holding them access the user's data in Firefox Sync
    /// and/or other FxA services. Applications should take care to store the resulting
    /// data in a secure fashion, as appropriate for their target platform.
    #[handle_error(Error)]
    pub fn to_json(&self) -> ApiResult<String> {
        self.internal.lock().to_json()
    }
}

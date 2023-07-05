/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # State management
//!
//! These are methods for managing the signed-in state of the application,
//! either by restoring a previously-saved state via [`FirefoxAccount::from_json`]
//! or by starting afresh with [`FirefoxAccount::new`].
//!
//! Applications can register for state changes using the [FirefoxAccount::register_storage_handler]
//! method.  After that, whenever the account state changes, the [StorageHandler::save_state]
//! method will be called.
//!
//! After calling any such method, use [`FirefoxAccount::to_json`] to serialize
//! the modified account state and persist the resulting string in application
//! settings.

use crate::{internal, ApiResult, CallbackResult, Error, FirefoxAccount};
use error_support::handle_error;
use parking_lot::Mutex;

impl FirefoxAccount {
    /// Restore a [`FirefoxAccount`] instance from serialized state.
    ///
    /// Given a JSON string previously passed to [StorageHandler.saved_state()], this
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

    /// Register a StorageHandler callback
    ///
    /// Any previously registered storage handler will be replaced.  Pass in None to clear out any
    /// storage handler.
    pub fn register_storage_handler(&self, handler: Option<Box<dyn StorageHandler>>) {
        self.internal.lock().state.register_storage_handler(handler)
    }

    /// Save current state to a JSON string
    ///
    /// **Deprecated**: Use the [FirefoxAccount::register_storage_handler()] method instead.
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

/// Handles storage for a FirefoxAccount.  This is implemented by the consumer application which
/// typically saves the FirefoxAccount state to secure storage on the device.
pub trait StorageHandler: Send + Sync {
    // This is called whenever the saved state changes.  The StorageHandler should ensure that the
    // state is saved to disk.  The next time the FirefoxAccount is constructed, it should be
    // through `from_json()` with this json data.
    //
    // **⚠️ Warning:** the serialized state may contain encryption keys and access
    // tokens that let anyone holding them access the user's data in Firefox Sync
    // and/or other FxA services. Applications should take care to store the resulting
    // data in a secure fashion, as appropriate for their target platform.
    fn save_state(&self, json: String) -> CallbackResult<()>;
}

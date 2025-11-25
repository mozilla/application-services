/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

#[macro_use]
mod error;
mod login;

mod db;
pub mod encryption;
mod schema;
mod store;
mod sync;
mod util;

use crate::encryption::{
    EncryptorDecryptor, KeyManager, ManagedEncryptorDecryptor, StaticKeyManager,
};
uniffi::include_scaffolding!("logins");

#[cfg(feature = "keydb")]
pub use crate::encryption::{NSSKeyManager, PrimaryPasswordAuthenticator};

pub use crate::db::{LoginDb, LoginsDeletionMetrics};
use crate::encryption::{check_canary, create_canary, create_key};
pub use crate::error::*;
pub use crate::login::*;
pub use crate::store::*;
pub use crate::sync::LoginsSyncEngine;
use std::sync::Arc;

// Utility function to create a StaticKeyManager to be used for the time being until support lands
// for [trait implementation of an UniFFI
// interface](https://mozilla.github.io/uniffi-rs/next/proc_macro/index.html#structs-implementing-traits)
// in UniFFI.
pub fn create_static_key_manager(key: String) -> Arc<StaticKeyManager> {
    Arc::new(StaticKeyManager::new(key))
}

// Similar to create_static_key_manager above, create a
// ManagedEncryptorDecryptor by passing in a KeyManager
pub fn create_managed_encdec(key_manager: Arc<dyn KeyManager>) -> Arc<ManagedEncryptorDecryptor> {
    Arc::new(ManagedEncryptorDecryptor::new(key_manager))
}

// Create a LoginStore by passing in a db path and a static key
//
// Note this is only temporarily needed until a bug with UniFFI and JavaScript is fixed, which
// prevents passing around traits in JS
pub fn create_login_store_with_static_key_manager(path: String, key: String) -> Arc<LoginStore> {
    let encdec: ManagedEncryptorDecryptor =
        ManagedEncryptorDecryptor::new(Arc::new(StaticKeyManager::new(key)));
    let store = LoginStore::new(path, Arc::new(encdec)).expect("error setting up LoginStore");
    Arc::new(store)
}

// Create a LoginStore with NSSKeyManager by passing in a db path and a PrimaryPasswordAuthenticator.
//
// Note this is only temporarily needed until a bug with UniFFI and JavaScript is fixed, which
// prevents passing around traits in JS
#[cfg(feature = "keydb")]
#[uniffi::export]
pub fn create_login_store_with_nss_keymanager(
    path: String,
    primary_password_authenticator: Arc<dyn PrimaryPasswordAuthenticator>,
) -> ApiResult<Arc<LoginStore>> {
    let encdec: ManagedEncryptorDecryptor = ManagedEncryptorDecryptor::new(Arc::new(
        NSSKeyManager::new(primary_password_authenticator),
    ));
    let store = LoginStore::new(path, Arc::new(encdec))?;
    Ok(Arc::new(store))
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

#[macro_use]
mod error;
mod login;

mod db;
mod schema;
mod store;
mod sync;
mod util;

use db_crypto::{EncryptorDecryptor, KeyManager, ManagedEncryptorDecryptor, StaticKeyManager};
uniffi::include_scaffolding!("logins");

#[cfg(feature = "keydb")]
pub use db_crypto::{NSSKeyManager, PrimaryPasswordAuthenticator};

pub use crate::db::{LoginDb, LoginsDeletionMetrics};
pub use crate::error::*;
pub use crate::login::*;
pub use crate::store::*;
pub use crate::sync::{LoginsBridgedEngine, LoginsSyncEngine};
use std::sync::Arc;

/// Identifier for the logins key, under which the key is stored in NSS.
#[cfg(feature = "keydb")]
static KEY_NAME: &str = "as-logins-key";

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
        NSSKeyManager::new(KEY_NAME.to_string(), primary_password_authenticator),
    ));
    let store = LoginStore::new(path, Arc::new(encdec))?;
    Ok(Arc::new(store))
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use serde::{de::DeserializeOwned, Serialize};

    lazy_static::lazy_static! {
        pub static ref TEST_ENCRYPTION_KEY: String = serde_json::to_string(&jwcrypto::Jwk::new_direct_key(Some("test-key".to_string())).unwrap()).unwrap();
        pub static ref TEST_ENCDEC: Arc<ManagedEncryptorDecryptor> = Arc::new(ManagedEncryptorDecryptor::new(Arc::new(StaticKeyManager::new(TEST_ENCRYPTION_KEY.clone()))));
    }

    pub fn encrypt_struct<T: Serialize>(fields: &T) -> String {
        let string = serde_json::to_string(fields).unwrap();
        let cipherbytes = TEST_ENCDEC.encrypt(string.as_bytes().into()).unwrap();
        std::str::from_utf8(&cipherbytes).unwrap().to_owned()
    }
    pub fn decrypt_struct<T: DeserializeOwned>(ciphertext: String) -> T {
        let jsonbytes = TEST_ENCDEC.decrypt(ciphertext.as_bytes().into()).unwrap();
        serde_json::from_str(std::str::from_utf8(&jsonbytes).unwrap()).unwrap()
    }
}

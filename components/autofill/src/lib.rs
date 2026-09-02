/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

pub mod db;
pub mod error;
pub mod sync;

// Re-export stuff the sync manager needs.
pub use crate::db::store::get_registered_sync_engine;

// Expose stuff needed by the uniffi generated code.
use crate::db::credit_cards::CreditCardsDeletionMetrics;
use crate::db::models::address::*;
use crate::db::models::credit_card::*;
use crate::db::models::passport::*;
use crate::db::store::Store;
pub use crate::sync::AddressesBridgedEngine;
use db_crypto::{EncryptorDecryptor, KeyManager, ManagedEncryptorDecryptor, StaticKeyManager};
pub use error::{ApiResult, AutofillApiError, Error, Result};
use error_support::handle_error;
use std::sync::Arc;

uniffi::include_scaffolding!("autofill");

#[cfg(feature = "keydb")]
pub use db_crypto::{NSSKeyManager, PrimaryPasswordAuthenticator};

/// Identifier for the autofill key, under which the key is stored in NSS.
#[cfg(feature = "keydb")]
static KEY_NAME: &str = "as-autofill-key";

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

// Create a Store by passing in a db path and a static key
//
// Note this is only temporarily needed until a bug with UniFFI and JavaScript is fixed, which
// prevents passing around traits in JS
pub fn create_autofill_store_with_static_key_manager(path: String, key: String) -> Arc<Store> {
    let encdec: ManagedEncryptorDecryptor =
        ManagedEncryptorDecryptor::new(Arc::new(StaticKeyManager::new(key)));
    let store = Store::new(path, Arc::new(encdec)).expect("error setting up Store");
    Arc::new(store)
}

// Create a Store with NSSKeyManager by passing in a db path and a PrimaryPasswordAuthenticator.
//
// Note this is only temporarily needed until a bug with UniFFI and JavaScript is fixed, which
// prevents passing around traits in JS
#[cfg(feature = "keydb")]
#[uniffi::export]
pub fn create_autofill_store_with_nss_keymanager(
    path: String,
    primary_password_authenticator: Arc<dyn PrimaryPasswordAuthenticator>,
) -> ApiResult<Arc<Store>> {
    let encdec: ManagedEncryptorDecryptor = ManagedEncryptorDecryptor::new(Arc::new(
        NSSKeyManager::new(KEY_NAME.to_string(), primary_password_authenticator),
    ));
    let store = Store::new(path, Arc::new(encdec))?;
    Ok(Arc::new(store))
}

// TODO(FXCM-2282): only `encrypt_string` and `decrypt_string` still build an
// encryptor from a key. When those go, so does this.
pub(crate) fn static_key_encryptor(key: &str) -> Result<ManagedEncryptorDecryptor> {
    // Validate eagerly so an invalid key isn't treated as undecryptable card data.
    jwcrypto::EncryptorDecryptor::new(key)?;

    Ok(ManagedEncryptorDecryptor::new(Arc::new(
        StaticKeyManager::new(key.to_string()),
    )))
}

// public functions we expose over the FFI (which is why they take `String`
// rather than the `&str` you'd otherwise expect)
#[handle_error(Error)]
pub fn encrypt_string(key: String, cleartext: String) -> ApiResult<String> {
    // It would be nice to have more detailed error messages, but that would require the consumer
    // to pass them in.  Let's not change the API yet.
    SecureCreditCardFields {
        cc_number: cleartext,
        ..Default::default()
    }
    .encrypt(&static_key_encryptor(&key)?, "<no guid>")
}

#[handle_error(Error)]
pub fn decrypt_string(key: String, ciphertext: String) -> ApiResult<String> {
    // It would be nice to have more detailed error messages, but that would require the consumer
    // to pass them in.  Let's not change the API yet.
    Ok(
        SecureCreditCardFields::decrypt(&ciphertext, &static_key_encryptor(&key)?, "<no guid>")?
            .cc_number,
    )
}

#[handle_error(Error)]
pub fn create_autofill_key() -> ApiResult<String> {
    Ok(db_crypto::create_key()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::credit_card::{decrypt_str, encrypt_str};
    use nss_as::ensure_initialized;

    #[test]
    fn test_encrypt() {
        ensure_initialized();
        let ed = static_key_encryptor(&create_autofill_key().unwrap()).unwrap();
        let cleartext = "secret";
        let ciphertext = encrypt_str(&ed, cleartext).unwrap();
        assert_eq!(decrypt_str(&ed, &ciphertext).unwrap(), cleartext);
        let ed2 = static_key_encryptor(&create_autofill_key().unwrap()).unwrap();
        assert!(matches!(
            decrypt_str(&ed2, &ciphertext),
            Err(Error::EncryptionError(
                db_crypto::DbCryptoApiError::DecryptionFailed { .. }
            ))
        ));
    }

    #[test]
    fn test_decryption_errors() {
        // The shared crate maps all jwcrypto decryption failures to DecryptionFailed.
        ensure_initialized();
        let ed = static_key_encryptor(&create_autofill_key().unwrap()).unwrap();
        assert!(matches!(
            decrypt_str(&ed, "invalid-ciphertext"),
            Err(Error::EncryptionError(
                db_crypto::DbCryptoApiError::DecryptionFailed { .. }
            )),
        ));
        assert!(matches!(
            decrypt_str(&ed, ""),
            Err(Error::EncryptionError(
                db_crypto::DbCryptoApiError::DecryptionFailed { .. }
            )),
        ));
    }

    #[test]
    fn test_an_invalid_key_is_rejected_up_front() {
        ensure_initialized();
        assert!(static_key_encryptor("not-a-key").is_err());
    }
}

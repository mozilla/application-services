/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

// This is the *local* encryption support - it has nothing to do with the
// encryption used by sync.

// For context, what "local encryption" means in this context is:
// * We use regular sqlite, but want to ensure the credit-card numbers are
//   encrypted in the DB - so we store the number encrypted, and the key
//   is managed by the app.
// * The app hands us an `EncryptorDecryptor` when it builds the store, and the
//   db holds on to it. Everything that reads or writes an encrypted column
//   takes it from the db. The core storage API never knows the unencrypted
//   number.
//
// This makes life tricky for Sync - sync has its own encryption and its own
// management of sync keys. The entire records are encrypted on the server -
// so the record on the server has the plain-text number (which is then
// encrypted as part of the entire record), so:
// * When transforming a record from the DB into a Sync record, we need to
//   *decrypt* the field.
// * When transforming a record from Sync into a DB record, we need to *encrypt*
//   the field.
//
// The sync code takes the encryptor from the store it already holds.

use crate::db::models::credit_card::SecureCreditCardFields;
use crate::error::*;
use error_support::handle_error;
use std::sync::Arc;

pub use db_crypto::{EncryptorDecryptor, KeyManager, ManagedEncryptorDecryptor, StaticKeyManager};

// TODO(FXCM-2282): only `encrypt_string` and `decrypt_string` still build an
// encryptor from a key. When those go, so does this.
pub(crate) fn static_key_encryptor(key: &str) -> Result<ManagedEncryptorDecryptor> {
    // Validate eagerly so an invalid key isn't treated as undecryptable card data.
    jwcrypto::EncryptorDecryptor::new(key)?;

    Ok(ManagedEncryptorDecryptor::new(Arc::new(
        StaticKeyManager::new(key.to_string()),
    )))
}

#[cfg(test)]
pub(crate) fn random_key_encryptor() -> Result<ManagedEncryptorDecryptor> {
    static_key_encryptor(&db_crypto::create_key()?)
}

pub(crate) fn encrypt_str(encdec: &dyn EncryptorDecryptor, cleartext: &str) -> Result<String> {
    let ciphertext = encdec.encrypt(cleartext.as_bytes().to_vec())?;
    String::from_utf8(ciphertext).map_err(|e| Error::CryptoNotUtf8(format!("encrypting: {e}")))
}

pub(crate) fn decrypt_str(encdec: &dyn EncryptorDecryptor, ciphertext: &str) -> Result<String> {
    let cleartext = encdec.decrypt(ciphertext.as_bytes().to_vec())?;
    String::from_utf8(cleartext).map_err(|e| Error::CryptoNotUtf8(format!("decrypting: {e}")))
}

// public functions we expose over the FFI (which is why they take `String`
// rather than the `&str` you'd otherwise expect)
#[handle_error(Error)]
pub fn encrypt_string(key: String, cleartext: String) -> ApiResult<String> {
    // It would be nice to have more detailed error messages, but that would require the consumer
    // to pass them in.  Let's not change the API yet.
    SecureCreditCardFields {
        cc_number: cleartext,
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
mod test {
    use super::*;
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

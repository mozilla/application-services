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
// * The credit-card API always just accepts and returns the encrypted string,
//   so we also expose encryption and decryption public functions that take
//   the key and text. The core storage API never knows the unencrypted number.
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
// So Sync needs to know the key etc, and that needs to get passed down
// multiple layers, from the app saying "sync now" all the way down to the
// low level sync code.
// To make life a little easier, we do that via a struct.

use crate::error::*;
use error_support::handle_error;

// Rather than passing keys around everywhere we abstract the encryption
// and decryption behind this struct.
pub struct EncryptorDecryptor {
    jwk: jwcrypto::Jwk,
}

impl EncryptorDecryptor {
    pub fn new(key: &str) -> Result<Self> {
        Ok(EncryptorDecryptor {
            jwk: serde_json::from_str(key)?,
        })
    }

    // For tests.
    #[cfg(test)]
    pub fn new_test_key() -> Self {
        let jwk = jwcrypto::Jwk::new_direct_key(Some("test-key".to_string())).unwrap();
        Self { jwk }
    }

    pub fn encrypt(&self, cleartext: &str) -> Result<String> {
        Ok(jwcrypto::encrypt_to_jwe(
            cleartext.as_bytes(),
            jwcrypto::EncryptionParameters::Direct {
                enc: jwcrypto::EncryptionAlgorithm::A256GCM,
                jwk: &self.jwk,
            },
        )?)
    }

    pub fn decrypt(&self, ciphertext: &str) -> Result<String> {
        if ciphertext.is_empty() {
            return Err(Error::EmptyCyphertext);
        }
        Ok(jwcrypto::decrypt_jwe(
            ciphertext,
            jwcrypto::DecryptionParameters::Direct {
                jwk: self.jwk.clone(),
            },
        )?)
    }
}

// public functions we expose over the FFI (which is why they take `String`
// rather than the `&str` you'd otherwise expect)
pub fn encrypt_string(key: String, cleartext: String) -> ApiResult<String> {
    handle_error! {
        EncryptorDecryptor::new(&key)?.encrypt(&cleartext)
    }
}

pub fn decrypt_string(key: String, ciphertext: String) -> ApiResult<String> {
    handle_error! {
        EncryptorDecryptor::new(&key)?.decrypt(&ciphertext)
    }
}

pub fn create_autofill_key() -> ApiResult<String> {
    handle_error! {
        let key = jwcrypto::Jwk::new_direct_key(None)?;
        Ok(serde_json::to_string(&key)?)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_encrypt() {
        let ed = EncryptorDecryptor::new(&create_autofill_key().unwrap()).unwrap();
        let cleartext = "secret";
        let ciphertext = ed.encrypt(cleartext).unwrap();
        assert_eq!(ed.decrypt(&ciphertext).unwrap(), cleartext);
        let ed2 = EncryptorDecryptor::new(&create_autofill_key().unwrap()).unwrap();
        assert!(matches!(
            ed2.decrypt(&ciphertext),
            Err(Error::CryptoError(_))
        ));
    }

    #[test]
    fn test_decryption_errors() {
        let ed = EncryptorDecryptor::new(&create_autofill_key().unwrap()).unwrap();
        assert!(matches!(
            ed.decrypt("invalid-ciphertext").unwrap_err(),
            Error::CryptoError(_)
        ));
        assert!(matches!(
            ed.decrypt("").unwrap_err(),
            Error::EmptyCyphertext
        ));
    }
}

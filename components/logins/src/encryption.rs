/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

// This is the *local* encryption support - it has nothing to do with the
// encryption used by sync.

// For context, what "local encryption" means in this context is:
// * We use regular sqlite, but ensure that sensitive data is encrypted in the DB in the
//   `secure_fields` column.  The encryption key is managed by the app.
// * The `decrypt_struct` and `encrypt_struct` functions are used to convert between an encrypted
//   `secure_fields` string and a decrypted `SecureFields` struct
// * Most API functions return `EncryptedLogin` which has its data encrypted.
//
// This makes life tricky for Sync - sync has its own encryption and its own
// management of sync keys. The entire records are encrypted on the server -
// so the record on the server has the plain-text data (which is then
// encrypted as part of the entire record), so:
// * When transforming a record from the DB into a Sync record, we need to
//   *decrypt* the data.
// * When transforming a record from Sync into a DB record, we need to *encrypt*
//   the data.
//
// So Sync needs to know the key etc, and that needs to get passed down
// multiple layers, from the app saying "sync now" all the way down to the
// low level sync code.
// To make life a little easier, we do that via a struct.

use crate::error::{ApiResult, Result};
use error_support::handle_error;
use serde::{de::DeserializeOwned, Serialize};

// Rather than passing keys around everywhere we abstract the encryption
// and decryption behind this struct.
#[derive(Clone, Debug)]
pub struct EncryptorDecryptor {
    jwk: jwcrypto::Jwk,
}

impl EncryptorDecryptor {
    /// Create a new EncryptorDecryptor
    pub fn new() -> ApiResult<Self> {
        handle_error! {
            Ok(Self {
                jwk: jwcrypto::Jwk::new_direct_key(None)?
            })
        }
    }

    /// Create an EncryptorDecryptor from an encryption key
    pub fn from_key(key: &str) -> ApiResult<Self> {
        handle_error! {
            Self::_from_key(key)
        }
    }

    /// Get the encryption key for an EncryptorDecryptor
    ///
    /// This can be used to reconstruct it later with create_from_key()
    pub fn get_key(&self) -> ApiResult<String> {
        handle_error! {
            self._get_key()
        }
    }

    /// Version of from_key for use internally in this crate
    ///
    /// This one returns a `Result` rather than an `ApiResult`
    pub(crate) fn _from_key(key: &str) -> Result<Self> {
        Ok(EncryptorDecryptor {
            jwk: serde_json::from_str(key)?,
        })
    }

    /// Version of from_key for use internally in this crate
    ///
    /// This one returns a `Result` rather than an `ApiResult`
    pub(crate) fn _get_key(&self) -> Result<String> {
        Ok(serde_json::to_string(&self.jwk)?)
    }

    // Encrypt a string.
    pub fn encrypt(&self, cleartext: &str) -> Result<String> {
        Ok(jwcrypto::encrypt_to_jwe(
            cleartext.as_bytes(),
            jwcrypto::EncryptionParameters::Direct {
                enc: jwcrypto::EncryptionAlgorithm::A256GCM,
                jwk: &self.jwk,
            },
        )?)
    }

    // Decrypt a string encrypted with `encrypt()`
    pub fn decrypt(&self, ciphertext: &str) -> Result<String> {
        Ok(jwcrypto::decrypt_jwe(
            ciphertext,
            jwcrypto::DecryptionParameters::Direct {
                jwk: self.jwk.clone(),
            },
        )?)
    }

    // Encrypt a struct, using serde_json to serialize it
    pub fn encrypt_struct<T: Serialize>(&self, value: &T) -> Result<String> {
        self.encrypt(&serde_json::to_string(value)?)
    }

    // Decrypt a struct encrypted with `encrypt_struct()`
    pub fn decrypt_struct<T: DeserializeOwned>(&self, ciphertext: &str) -> Result<T> {
        Ok(serde_json::from_str(&self.decrypt(ciphertext)?)?)
    }

    // Create a "canary" string, which can be used to test if the encryption key is still valid for the logins data
    pub fn create_canary(&self, text: &str) -> ApiResult<String> {
        handle_error! {
            self.encrypt(text)
        }
    }

    // Check that key is still valid using the output of `create_canary`
    //
    // `text` much match the text you initially passed to `create_canary()`
    pub fn check_canary(&self, canary: &str, text: &str) -> bool {
        match self.decrypt(canary) {
            Ok(decrypted) => decrypted == text,
            Err(_) => false,
        }
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::*;

    lazy_static::lazy_static! {
        pub static ref TEST_ENCRYPTOR: EncryptorDecryptor = EncryptorDecryptor::new().unwrap();
    }

    pub fn encrypt(value: &str) -> String {
        TEST_ENCRYPTOR.encrypt(value).unwrap()
    }
    pub fn decrypt(value: &str) -> String {
        TEST_ENCRYPTOR.decrypt(value).unwrap()
    }
    pub fn encrypt_struct<T: Serialize>(fields: &T) -> String {
        TEST_ENCRYPTOR.encrypt_struct(fields).unwrap()
    }
    pub fn decrypt_struct<T: DeserializeOwned>(ciphertext: String) -> T {
        TEST_ENCRYPTOR.decrypt_struct(&ciphertext).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn string_encryption() {
        let encdec = EncryptorDecryptor::new().unwrap();
        assert_eq!(
            encdec
                .encrypt("test-string")
                .and_then(|ciphertext| encdec.decrypt(&ciphertext))
                .unwrap(),
            "test-string"
        );
    }

    #[test]
    fn struct_encryption() {
        #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
        struct TestStruct {
            a: String,
            b: i32,
        }

        let value = TestStruct {
            a: String::from("Foo"),
            b: 123,
        };
        let encdec = EncryptorDecryptor::new().unwrap();
        assert_eq!(
            encdec
                .encrypt_struct(&value)
                .and_then(|ciphertext| encdec.decrypt_struct::<TestStruct>(&ciphertext))
                .unwrap(),
            value
        );
    }

    #[test]
    fn canary_check() {
        let encdec = EncryptorDecryptor::new().unwrap();
        let canary = encdec.create_canary("MyCanaryText").unwrap();
        assert!(encdec.check_canary(&canary, "MyCanaryText"));
        assert!(!encdec.check_canary(&canary, "SomeOtherCanaryText"));
        assert!(!encdec.check_canary("some-other-canary", "MyCanaryText"));
    }
}

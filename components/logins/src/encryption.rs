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

use crate::error::*;
use serde::{de::DeserializeOwned, Serialize};

// Rather than passing keys around everywhere we abstract the encryption
// and decryption behind this struct.
pub struct EncryptorDecryptor {
    jwk: jwcrypto::Jwk,
}

impl EncryptorDecryptor {
    pub fn new(key: &str) -> Result<Self> {
        match serde_json::from_str(key) {
            Ok(jwk) => Ok(EncryptorDecryptor { jwk }),
            Err(_) => Err(LoginsError::InvalidKey),
        }
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

    pub fn encrypt_struct<T: Serialize>(&self, fields: &T) -> Result<String> {
        let str = serde_json::to_string(fields)?;
        self.encrypt(&str)
    }

    pub fn decrypt(&self, ciphertext: &str) -> Result<String> {
        Ok(jwcrypto::decrypt_jwe(
            ciphertext,
            jwcrypto::DecryptionParameters::Direct {
                jwk: self.jwk.clone(),
            },
        )?)
    }

    pub fn decrypt_struct<T: DeserializeOwned>(&self, ciphertext: &str) -> Result<T> {
        let json = self.decrypt(ciphertext)?;
        Ok(serde_json::from_str(&json)?)
    }
}

// Canary checking functions.  These are used to check if a key is still valid for a database.  The
// basic process is:
//   - When opening a database the first time, store the output of `create_canary()` alongside the DB
//     and encryption key.
//   - When reopening the database, check that the encryption key is still valid for the canary
//     text using `check_canary()`
//     - If it returns true, then it's safe to assume the key can decrypt the DB data
//     - If it returns false, then the key is no longer valid.  It should be regenerated and the DB
//       data should be wiped since we can no longer read it properly
pub fn create_canary(text: &str, key: &str) -> ApiResult<String> {
    handle_error! {
        EncryptorDecryptor::new(key)?.encrypt(text)
    }
}

pub fn check_canary(canary: &str, text: &str, key: &str) -> ApiResult<bool> {
    handle_error! {
        Ok(EncryptorDecryptor::new(key)?.decrypt(canary)? == text)
    }
}

pub fn create_key() -> ApiResult<String> {
    handle_error! {
        let key = jwcrypto::Jwk::new_direct_key(None)?;
        Ok(serde_json::to_string(&key)?)
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::*;

    lazy_static::lazy_static! {
        pub static ref TEST_ENCRYPTION_KEY: String = serde_json::to_string(&jwcrypto::Jwk::new_direct_key(Some("test-key".to_string())).unwrap()).unwrap();
        pub static ref TEST_ENCRYPTOR: EncryptorDecryptor = EncryptorDecryptor::new(&TEST_ENCRYPTION_KEY).unwrap();
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
mod test {
    use super::*;

    #[test]
    fn test_encrypt() {
        let ed = EncryptorDecryptor::new(&create_key().unwrap()).unwrap();
        let cleartext = "secret";
        let ciphertext = ed.encrypt(cleartext).unwrap();
        assert_eq!(ed.decrypt(&ciphertext).unwrap(), cleartext);
        let ed2 = EncryptorDecryptor::new(&create_key().unwrap()).unwrap();
        assert!(matches!(
            ed2.decrypt(&ciphertext).err().unwrap(),
            LoginsError::CryptoError(_)
        ));
    }

    #[test]
    fn test_key_error() {
        let storage_err = EncryptorDecryptor::new("bad-key").err().unwrap();
        assert!(matches!(storage_err, LoginsError::InvalidKey));
    }

    #[test]
    fn test_canary_functionality() {
        const CANARY_TEXT: &str = "Arbitrary sequence of text";
        let key = create_key().unwrap();
        let canary = create_canary(CANARY_TEXT, &key).unwrap();
        assert!(check_canary(&canary, CANARY_TEXT, &key).unwrap());

        let different_key = create_key().unwrap();
        assert!(matches!(
            check_canary(&canary, CANARY_TEXT, &different_key)
                .err()
                .unwrap(),
            LoginsStorageError::IncorrectKey
        ));
    }
}

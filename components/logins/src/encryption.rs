/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

// This is the *local* encryption support - it has nothing to do with the
// encryption used by sync.

// For context, what "local encryption" means in this context is:
// * We use regular sqlite, but want to ensure the sensitive data is
//   encrypted in the DB - so we store the data encrypted, and the key
//   is managed by the app.
// * The API always just accepts and returns the encrypted strings, so we
//   also expose encryption and decryption public functions that take the
//   key and text. The core storage API never knows the unencrypted number.
//
// This makes life tricky for Sync - sync has its own encryption and its own
// management of sync keys. The entire records are encrypted on the server -
// so the record on the server has the plain-text data (which is then
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
            Err(_) => Err(ErrorKind::InvalidKey.into()),
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
            &ciphertext,
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

// public functions we expose over the FFI (which is why they take `String`
// rather than the `&str` you'd otherwise expect)
pub fn encrypt_string(key: String, cleartext: String) -> Result<String> {
    EncryptorDecryptor::new(&key)?.encrypt(&cleartext)
}

pub fn decrypt_string(key: String, ciphertext: String) -> Result<String> {
    EncryptorDecryptor::new(&key)?.decrypt(&ciphertext)
}

pub fn create_key() -> Result<String> {
    let key = jwcrypto::Jwk::new_direct_key(None)?;
    Ok(serde_json::to_string(&key)?)
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
            ed2.decrypt(&ciphertext).err().unwrap().kind(),
            ErrorKind::CryptoError(_)
        ));
    }

    #[test]
    fn test_key_error() {
        let storage_err: LoginsStorageError =
            EncryptorDecryptor::new("bad-key").err().unwrap().into();
        assert!(matches!(storage_err, LoginsStorageError::InvalidKey(_)));
    }
}

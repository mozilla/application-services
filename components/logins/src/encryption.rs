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

pub type EncryptorDecryptor = jwcrypto::EncryptorDecryptor<Error>;

#[handle_error(Error)]
pub fn create_canary(text: &str, key: &str) -> ApiResult<String> {
    EncryptorDecryptor::new(key)?.create_canary(text)
}

#[handle_error(Error)]
pub fn check_canary(canary: &str, text: &str, key: &str) -> ApiResult<bool> {
    EncryptorDecryptor::new(key)?.check_canary(canary, text)
}

#[handle_error(Error)]
pub fn create_key() -> ApiResult<String> {
    EncryptorDecryptor::create_key()
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use serde::{de::DeserializeOwned, Serialize};

    lazy_static::lazy_static! {
        pub static ref TEST_ENCRYPTION_KEY: String = serde_json::to_string(&jwcrypto::Jwk::new_direct_key(Some("test-key".to_string())).unwrap()).unwrap();
        pub static ref TEST_ENCRYPTOR: EncryptorDecryptor = EncryptorDecryptor::new(&TEST_ENCRYPTION_KEY).unwrap();
    }
    pub fn encrypt(value: &str) -> String {
        TEST_ENCRYPTOR.encrypt(value, "test encrypt").unwrap()
    }
    pub fn decrypt(value: &str) -> String {
        TEST_ENCRYPTOR.decrypt(value, "test decrypt").unwrap()
    }
    pub fn encrypt_struct<T: Serialize>(fields: &T) -> String {
        TEST_ENCRYPTOR
            .encrypt_struct(fields, "test encrypt struct")
            .unwrap()
    }
    pub fn decrypt_struct<T: DeserializeOwned>(ciphertext: String) -> T {
        TEST_ENCRYPTOR
            .decrypt_struct(&ciphertext, "test decrypt struct")
            .unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_encrypt() {
        let ed = EncryptorDecryptor::new(&create_key().unwrap()).unwrap();
        let cleartext = "secret";
        let ciphertext = ed.encrypt(cleartext, "test encrypt").unwrap();
        assert_eq!(ed.decrypt(&ciphertext, "test decrypt").unwrap(), cleartext);
        let ed2 = EncryptorDecryptor::new(&create_key().unwrap()).unwrap();
        assert!(matches!(
            ed2.decrypt(&ciphertext, "test decrypt").err().unwrap(),
            Error::CryptoError(jwcrypto::EncryptorDecryptorError { description, .. })
            if description == "test decrypt"
        ));
    }

    #[test]
    fn test_key_error() {
        let storage_err = EncryptorDecryptor::new("bad-key").err().unwrap();
        assert!(matches!(
            storage_err,
            Error::CryptoError(jwcrypto::EncryptorDecryptorError {
                from: jwcrypto::JwCryptoError::InvalidKey,
                ..
            })
        ));
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
            LoginsApiError::IncorrectKey
        ));
    }
}

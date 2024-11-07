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
//
// Consumers of the Login component have 3 options for setting up encryption:
//    1. Implement EncryptorDecryptor directly
//       eg `LoginStore::new(MyEncryptorDecryptor)`
//    2. Implement KeyManager and use ManagedEncryptorDecryptor
//       eg `LoginStore::new(ManagedEncryptorDecryptor::new(MyKeyManager))`
//    3. Generate a single key and create a StaticKeyManager and use it together with
//       ManagedEncryptorDecryptor
//       eg `LoginStore::new(ManagedEncryptorDecryptor::new(StaticKeyManager { key: myKey }))`
//
//  You can implement EncryptorDecryptor directly to keep full control over the encryption
//  algorithm. For example, on the desktop, this could make use of NSS's SecretDecoderRing to
//  achieve transparent key management.
//
//  If the application wants to keep the current encryption, like Android and iOS, for example, but
//  control the key management itself, the KeyManager can be implemented and the encryption can be
//  done on the Rust side with the ManagedEncryptorDecryptor.
//
//  In tests or some command line tools, it can be practical to use a static key that does not
//  change at runtime and is already present when the LoginsStore is initialized. In this case, it
//  makes sense to use the provided StaticKeyManager.

use crate::error::*;
use std::sync::Arc;

/// This is the generic EncryptorDecryptor trait, as handed over to the Store during initialization.
/// Consumers can implement either this generic trait and bring in their own crypto, or leverage the
/// ManagedEncryptorDecryptor below, which provides encryption algorithms out of the box.
///
/// Note that EncryptorDecryptor must not call any LoginStore methods. The login store can call out
/// to the EncryptorDecryptor when it's internal mutex is held so calling back in to the LoginStore
/// may deadlock.
pub trait EncryptorDecryptor: Send + Sync {
    fn encrypt(&self, cleartext: Vec<u8>) -> ApiResult<Vec<u8>>;
    fn decrypt(&self, ciphertext: Vec<u8>) -> ApiResult<Vec<u8>>;
}

impl<T: EncryptorDecryptor> EncryptorDecryptor for Arc<T> {
    fn encrypt(&self, clearbytes: Vec<u8>) -> ApiResult<Vec<u8>> {
        (**self).encrypt(clearbytes)
    }

    fn decrypt(&self, cipherbytes: Vec<u8>) -> ApiResult<Vec<u8>> {
        (**self).decrypt(cipherbytes)
    }
}

/// The ManagedEncryptorDecryptor makes use of the NSS provided cryptographic algorithms. The
/// ManagedEncryptorDecryptor uses a KeyManager for encryption key retrieval.
pub struct ManagedEncryptorDecryptor {
    key_manager: Arc<dyn KeyManager>,
}

impl ManagedEncryptorDecryptor {
    pub fn new(key_manager: Arc<dyn KeyManager>) -> Self {
        Self { key_manager }
    }
}

impl EncryptorDecryptor for ManagedEncryptorDecryptor {
    fn encrypt(&self, clearbytes: Vec<u8>) -> ApiResult<Vec<u8>> {
        let keybytes = self
            .key_manager
            .get_key()
            .map_err(|_| LoginsApiError::MissingKey)?;
        let key = std::str::from_utf8(&keybytes).map_err(|_| LoginsApiError::InvalidKey)?;

        let encdec = jwcrypto::EncryptorDecryptor::new(key)
            .map_err(|_: jwcrypto::EncryptorDecryptorError| LoginsApiError::InvalidKey)?;

        let cleartext =
            std::str::from_utf8(&clearbytes).map_err(|e| LoginsApiError::EncryptionFailed {
                reason: e.to_string(),
            })?;
        encdec
            .encrypt(cleartext, "encrypt SecureLoginFields")
            .map_err(
                |e: jwcrypto::EncryptorDecryptorError| LoginsApiError::EncryptionFailed {
                    reason: e.to_string(),
                },
            )
            .map(|text| text.into())
    }

    fn decrypt(&self, cipherbytes: Vec<u8>) -> ApiResult<Vec<u8>> {
        let keybytes = self
            .key_manager
            .get_key()
            .map_err(|_| LoginsApiError::MissingKey)?;
        let key = std::str::from_utf8(&keybytes).map_err(|_| LoginsApiError::InvalidKey)?;

        let encdec = jwcrypto::EncryptorDecryptor::new(key)
            .map_err(|_: jwcrypto::EncryptorDecryptorError| LoginsApiError::InvalidKey)?;

        let ciphertext =
            std::str::from_utf8(&cipherbytes).map_err(|e| LoginsApiError::DecryptionFailed {
                reason: e.to_string(),
            })?;
        encdec
            .decrypt(ciphertext, "decrypt SecureLoginFields")
            .map_err(
                |e: jwcrypto::EncryptorDecryptorError| LoginsApiError::DecryptionFailed {
                    reason: e.to_string(),
                },
            )
            .map(|text| text.into())
    }
}

/// Consumers can implement the KeyManager in combination with the ManagedEncryptorDecryptor to hand
/// over the encryption key whenever encryption or decryption happens.
pub trait KeyManager: Send + Sync {
    fn get_key(&self) -> ApiResult<Vec<u8>>;
}

/// Last but not least we provide a StaticKeyManager, which can be
/// used in cases where there is a single key during runtime, for example in tests.
pub struct StaticKeyManager {
    key: String,
}

impl StaticKeyManager {
    pub fn new(key: String) -> Self {
        Self { key }
    }
}

impl KeyManager for StaticKeyManager {
    #[handle_error(Error)]
    fn get_key(&self) -> ApiResult<Vec<u8>> {
        Ok(self.key.as_bytes().into())
    }
}

#[handle_error(Error)]
pub fn create_canary(text: &str, key: &str) -> ApiResult<String> {
    jwcrypto::EncryptorDecryptor::new(key)?.create_canary(text)
}

pub fn check_canary(canary: &str, text: &str, key: &str) -> ApiResult<bool> {
    let encdec = jwcrypto::EncryptorDecryptor::new(key)
        .map_err(|_: jwcrypto::EncryptorDecryptorError| LoginsApiError::InvalidKey)?;
    Ok(encdec.check_canary(canary, text).unwrap_or(false))
}

#[handle_error(Error)]
pub fn create_key() -> ApiResult<String> {
    jwcrypto::EncryptorDecryptor::create_key()
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use serde::{de::DeserializeOwned, Serialize};

    lazy_static::lazy_static! {
        pub static ref TEST_ENCRYPTION_KEY: String = serde_json::to_string(&jwcrypto::Jwk::new_direct_key(Some("test-key".to_string())).unwrap()).unwrap();
        pub static ref TEST_ENCDEC: Arc<ManagedEncryptorDecryptor> = Arc::new(ManagedEncryptorDecryptor::new(Arc::new(StaticKeyManager { key: TEST_ENCRYPTION_KEY.clone() })));
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_static_key_manager() {
        let key = create_key().unwrap();
        let key_manager = StaticKeyManager { key: key.clone() };
        assert_eq!(key.as_bytes(), key_manager.get_key().unwrap());
    }

    #[test]
    fn test_managed_encdec_with_invalid_key() {
        let key_manager = Arc::new(StaticKeyManager {
            key: "bad_key".to_owned(),
        });
        let encdec = ManagedEncryptorDecryptor { key_manager };
        assert!(matches!(
            encdec.encrypt("secret".as_bytes().into()).err().unwrap(),
            LoginsApiError::InvalidKey
        ));
    }

    #[test]
    fn test_managed_encdec_with_missing_key() {
        struct MyKeyManager {}
        impl KeyManager for MyKeyManager {
            fn get_key(&self) -> ApiResult<Vec<u8>> {
                Err(LoginsApiError::MissingKey)
            }
        }
        let key_manager = Arc::new(MyKeyManager {});
        let encdec = ManagedEncryptorDecryptor { key_manager };
        assert!(matches!(
            encdec.encrypt("secret".as_bytes().into()).err().unwrap(),
            LoginsApiError::MissingKey
        ));
    }

    #[test]
    fn test_managed_encdec() {
        let key = create_key().unwrap();
        let key_manager = Arc::new(StaticKeyManager { key });
        let encdec = ManagedEncryptorDecryptor { key_manager };
        let cleartext = "secret";
        let ciphertext = encdec.encrypt(cleartext.as_bytes().into()).unwrap();
        assert_eq!(
            encdec.decrypt(ciphertext.clone()).unwrap(),
            cleartext.as_bytes()
        );
        let other_encdec = ManagedEncryptorDecryptor {
            key_manager: Arc::new(StaticKeyManager {
                key: create_key().unwrap(),
            }),
        };
        assert!(matches!(
            other_encdec.decrypt(ciphertext).err().unwrap(),
            LoginsApiError::DecryptionFailed { reason: _ }
        ));
    }

    #[test]
    fn test_key_error() {
        let storage_err = jwcrypto::EncryptorDecryptor::new("bad-key").err().unwrap();
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
        assert!(!check_canary(&canary, CANARY_TEXT, &different_key).unwrap());

        let bad_key = "bad_key".to_owned();
        assert!(matches!(
            check_canary(&canary, CANARY_TEXT, &bad_key).err().unwrap(),
            LoginsApiError::InvalidKey
        ));
    }
}

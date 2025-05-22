/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{JwCryptoError, Jwk};
use serde::{de::DeserializeOwned, Serialize};

/// High-level struct for handling Encryption/Decryption
pub struct EncryptorDecryptor {
    jwk: Jwk,
}

impl EncryptorDecryptor {
    /// Create a key that can be used to construct an EncryptorDecryptor
    pub fn create_key() -> Result<String, JwCryptoError> {
        let key = crate::Jwk::new_direct_key(None)?;
        Ok(serde_json::to_string(&key)?)
    }

    pub fn new(key: &str) -> Result<Self, JwCryptoError> {
        match serde_json::from_str(key) {
            Ok(jwk) => Ok(Self { jwk }),
            Err(_) => Err(JwCryptoError::InvalidKey),
        }
    }

    pub fn new_with_random_key() -> Result<Self, JwCryptoError> {
        Self::new(&Self::create_key()?)
    }

    /// Encrypt a string
    ///
    /// `description` is a developer-friendly description of the operation that gets reported to Sentry
    /// on crypto errors.
    pub fn encrypt(&self, cleartext: &str) -> Result<String, JwCryptoError> {
        crate::encrypt_to_jwe(
            cleartext.as_bytes(),
            crate::EncryptionParameters::Direct {
                enc: crate::EncryptionAlgorithm::A256GCM,
                jwk: &self.jwk,
            },
        )
    }

    /// Encrypt a struct
    ///
    /// `description` is a developer-friendly description of the operation that gets reported to Sentry
    /// on crypto errors.
    pub fn encrypt_struct<T: Serialize>(&self, fields: &T) -> Result<String, JwCryptoError> {
        let str = serde_json::to_string(fields)?;
        self.encrypt(&str)
    }

    /// Decrypt a string
    ///
    /// `description` is a developer-friendly description of the operation that gets reported to Sentry
    /// on crypto errors.
    pub fn decrypt(&self, ciphertext: &str) -> Result<String, JwCryptoError> {
        if ciphertext.is_empty() {
            return Err(JwCryptoError::EmptyCyphertext);
        }
        crate::decrypt_jwe(
            ciphertext,
            crate::DecryptionParameters::Direct {
                jwk: self.jwk.clone(),
            },
        )
    }

    /// Decrypt a struct
    ///
    /// `description` is a developer-friendly description of the operation that gets reported to Sentry
    /// on crypto errors.
    pub fn decrypt_struct<T: DeserializeOwned>(
        &self,
        ciphertext: &str,
    ) -> Result<T, JwCryptoError> {
        let json = self.decrypt(ciphertext)?;
        Ok(serde_json::from_str(&json)?)
    }

    // Create canary text.
    //
    // These are used to check if a key is still valid for a database.  Call this when opening a
    // database for the first time and save the result.
    pub fn create_canary(&self, text: &str) -> Result<String, JwCryptoError> {
        self.encrypt(text)
    }

    // Create canary text.
    //
    // These are used to check if a key is still valid for a database.  Call this when re-opening a
    // database, using the same text parameter and the return value of the initial check_canary call.
    //
    // - If check_canary() returns true, then it's safe to assume the key can decrypt the DB data
    // - If check_canary() returns false, then the key is no longer valid.  It should be
    // regenerated and the DB data should be wiped since we can no longer read it properly
    pub fn check_canary(&self, canary: &str, text: &str) -> Result<bool, JwCryptoError> {
        Ok(self.decrypt(canary)? == text)
    }
}

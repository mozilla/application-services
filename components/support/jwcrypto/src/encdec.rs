/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{EncryptorDecryptorError, JwCryptoError, Jwk};
use serde::{de::DeserializeOwned, Serialize};
use std::marker::PhantomData;

/// High-level struct for handling Encryption/Decryption
///
/// This struct wraps the jwcrypto functionality in a convenient package.  Also, it uses
/// `EncryptorDecryptorError`, which includes a description that can help track down where
/// encryption errors are happening.
///
/// EncryptorDecryptor takes a generic Error parameter that it uses for its results.  This can be
/// anything that implement `From<EncryptorDecryptorError>`, typically it's the internal error type
/// for a crate.
pub struct EncryptorDecryptor<E = EncryptorDecryptorError> {
    jwk: Jwk,
    phantom: PhantomData<*const E>,
}

// Need to implement Send/Sync by hand, since technically we have a pointer field.  This is safe
// since we don't actually store the error value.
unsafe impl<E> Send for EncryptorDecryptor<E> {}

unsafe impl<E> Sync for EncryptorDecryptor<E> {}

impl<E: From<EncryptorDecryptorError>> EncryptorDecryptor<E> {
    /// Create a key that can be used to construct an EncryptorDecryptor
    pub fn create_key() -> Result<String, E> {
        let key = crate::Jwk::new_direct_key(None).to_encdec_result("create key")?;
        Ok(serde_json::to_string(&key).to_encdec_result("create_key (serialization)")?)
    }

    pub fn new(key: &str) -> Result<Self, E> {
        match serde_json::from_str(key) {
            Ok(jwk) => Ok(Self {
                jwk,
                phantom: PhantomData,
            }),
            Err(_) => Err(EncryptorDecryptorError {
                from: JwCryptoError::InvalidKey,
                description: "creating EncryptorDecryptor".into(),
            }
            .into()),
        }
    }

    pub fn new_with_random_key() -> Result<Self, E> {
        Self::new(&Self::create_key()?)
    }

    /// Encrypt a string
    ///
    /// `description` is a developer-friendly description of the operation that gets reported to Sentry
    /// on crypto errors.
    pub fn encrypt(&self, cleartext: &str, description: &str) -> Result<String, E> {
        crate::encrypt_to_jwe(
            cleartext.as_bytes(),
            crate::EncryptionParameters::Direct {
                enc: crate::EncryptionAlgorithm::A256GCM,
                jwk: &self.jwk,
            },
        )
        .to_encdec_result(description)
    }

    /// Encrypt a struct
    ///
    /// `description` is a developer-friendly description of the operation that gets reported to Sentry
    /// on crypto errors.
    pub fn encrypt_struct<T: Serialize>(&self, fields: &T, description: &str) -> Result<String, E> {
        let str = serde_json::to_string(fields).to_encdec_result(description)?;
        self.encrypt(&str, description)
    }

    /// Decrypt a string
    ///
    /// `description` is a developer-friendly description of the operation that gets reported to Sentry
    /// on crypto errors.
    pub fn decrypt(&self, ciphertext: &str, description: &str) -> Result<String, E> {
        if ciphertext.is_empty() {
            return Err(JwCryptoError::EmptyCyphertext).to_encdec_result(description);
        }
        crate::decrypt_jwe(
            ciphertext,
            crate::DecryptionParameters::Direct {
                jwk: self.jwk.clone(),
            },
        )
        .to_encdec_result(description)
    }

    /// Decrypt a struct
    ///
    /// `description` is a developer-friendly description of the operation that gets reported to Sentry
    /// on crypto errors.
    pub fn decrypt_struct<T: DeserializeOwned>(
        &self,
        ciphertext: &str,
        description: &str,
    ) -> Result<T, E> {
        let json = self.decrypt(ciphertext, description)?;
        Ok(serde_json::from_str(&json).to_encdec_result(description)?)
    }

    // Create canary text.
    //
    // These are used to check if a key is still valid for a database.  Call this when opening a
    // database for the first time and save the result.
    pub fn create_canary(&self, text: &str) -> Result<String, E> {
        self.encrypt(text, "create canary")
    }

    // Create canary text.
    //
    // These are used to check if a key is still valid for a database.  Call this when re-opening a
    // database, using the same text parameter and the return value of the initial check_canary call.
    //
    // - If check_canary() returns true, then it's safe to assume the key can decrypt the DB data
    // - If check_canary() returns false, then the key is no longer valid.  It should be
    // regenerated and the DB data should be wiped since we can no longer read it properly
    pub fn check_canary(&self, canary: &str, text: &str) -> Result<bool, E> {
        Ok(self.decrypt(canary, "check canary")? == text)
    }
}

trait ToEncryptorDecryptorResult<T, E> {
    fn to_encdec_result(self, description: &str) -> Result<T, E>;
}

impl<T, InternalError, ExternalError> ToEncryptorDecryptorResult<T, ExternalError>
    for Result<T, InternalError>
where
    InternalError: Into<JwCryptoError>,
    ExternalError: From<EncryptorDecryptorError>,
{
    fn to_encdec_result(self, description: &str) -> Result<T, ExternalError> {
        self.map_err(|e| {
            EncryptorDecryptorError {
                from: e.into(),
                description: description.into(),
            }
            .into()
        })
    }
}

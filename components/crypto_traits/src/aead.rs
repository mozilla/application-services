//! Authenticated Encryption with Additional Data (AEAD)
//!
//! AEAD creates a uniform interface for various underlying cryptographic algorithms, and makes it easy to enforce
//! both confidentiality and data integrity in a simple interface.
//!
//! To read more about AEAD, check out [RFC 5116](https://datatracker.ietf.org/doc/html/rfc5116)
//!
//!
//! This module exposes a trait that represents the AEAD interface described in the RFC.

use crate::error;

/// Implementors of this trait implement Authenticated Encryption with Additional Data.
pub trait Aead {
    /// [Authenticated Encryption](https://datatracker.ietf.org/doc/html/rfc5116#section-2.1)
    ///
    /// # Arguments
    /// - `key`: A secret key, which must be generated in a way that is uniformly random or pseudorandom
    /// - `nonce`: An invocation specific nonce, some invocation may not use a nonce
    /// - `plaintext`: The data to be encrypted
    /// - `associated_data`: The data to be authenticated
    ///
    /// # Returns
    /// Returns a [`Vec<u8>`] representing the ciphertext or an error
    fn seal(
        &self,
        algorithm: AeadAlgorithm,
        key: &[u8],
        nonce: Option<&[u8]>,
        plaintext: &[u8],
        associated_data: &[u8],
    ) -> error::Result<Vec<u8>>;

    /// [Authenticated Decryption](https://datatracker.ietf.org/doc/html/rfc5116#section-2.2)
    ///
    /// # Arguments
    /// - `key`: A secret key, which must be generated in a way that is uniformly random or pseudorandom
    /// - `nonce`: An invocation specific nonce, some invocation may not use a nonce
    /// - `ciphertext`: The data to be decrypted
    /// - `associated_data`: The data to be authenticated
    /// # Returns
    /// Returns a [`Vec<u8>`] representing the plaintext or an error
    fn open(
        &self,
        algorithm: AeadAlgorithm,
        key: &[u8],
        nonce: Option<&[u8]>,
        ciphertext: &[u8],
        associated_data: &[u8],
    ) -> error::Result<Vec<u8>>;
}

/// An AEAD Algorithm
///
/// The represents the concrete AEAD algorithms
/// ```
pub enum AeadAlgorithm {
    /// Represents an AES_128_GCM Aead algorithm as described
    /// in [RFC 5516, section 5.1](https://datatracker.ietf.org/doc/html/rfc5116#section-5.1)
    Aes128Gcm,
    /// Represents an AES_256_GCM Aead algorithm as described
    /// in [RFC 5516, section 5.1](https://datatracker.ietf.org/doc/html/rfc5116#section-5.2)
    Aes256Gcm,
    /// AES-256 in CBC mode with HMAC-SHA256 tags and 128 bit nonces.
    /// This is a Sync 1.5 specific encryption scheme, do not use for new
    /// applications, there are better options out there nowadays.
    /// Important note: The HMAC tag verification should be done against the
    /// base64 representation of the ciphertext.
    /// More details here: https://mozilla-services.readthedocs.io/en/latest/sync/storageformat5.html#record-encryption
    SyncAes256CBC,
}

impl AeadAlgorithm {
    pub fn key_len(&self) -> usize {
        match self {
            Self::Aes128Gcm => 16,
            Self::Aes256Gcm => 32,
            Self::SyncAes256CBC => 64,
        }
    }
    pub fn nonce_len(&self) -> usize {
        match self {
            Self::Aes128Gcm => 96 / 8,
            Self::Aes256Gcm => 96 / 8,
            Self::SyncAes256CBC => 128 / 8,
        }
    }
    pub fn tag_len(&self) -> usize {
        match self {
            Self::Aes128Gcm => 16,
            Self::Aes256Gcm => 16,
            Self::SyncAes256CBC => 32,
        }
    }
}

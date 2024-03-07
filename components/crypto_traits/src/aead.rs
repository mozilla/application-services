//! Authenticated Encryption with Additional Data (AEAD)
//!
//! AEAD creates a uniform interface for various underlying cryptographic algorithms, and makes it easy to enforce
//! both confidentiality and data integrity in a simple interface.
//!
//! To read more about AEAD, check out [RFC 5116](https://datatracker.ietf.org/doc/html/rfc5116)
//!
//!
//! This module exposes a trait that represents the AEAD interface described in the RFC.

/// Implementors of this trait implement Authenticated Encryption with Additional Data.
pub trait Aead<A>
where
    A: AeadAlgorithm,
{
    type Error: std::error::Error;
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
        key: &[u8],
        nonce: Option<&[u8]>,
        plaintext: &[u8],
        associated_data: &[u8],
    ) -> std::result::Result<Vec<u8>, Self::Error>;

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
        key: &[u8],
        nonce: Option<&[u8]>,
        ciphertext: &[u8],
        associated_data: &[u8],
    ) -> std::result::Result<Vec<u8>, Self::Error>;
}

/// An AEAD Algorithm
///
/// The represents a concrete AEAD algorithm so crypto backends can specify which
/// AEAD algorithms they support, and potentially provide their own algorithms
/// although they should do that with care.
///
/// Mostly, this trait is meant to be treated as a marker for implementors of
/// [`Aead`]
/// For example:
/// ```rs
/// use crypto_traits::aead::{Aead, AesGcm};
/// pub struct NSSCrypto {};
/// impl Aead<AesGcm> for NSSCrypto {
///     fn seal(&self,
///             key: &[u8],
///             nonce: &[u8],
///             plaintext: &[u8],
///             associated_data: &[u8]
///             ) -> Result<Vec<u8>, Self::Error> {
///         todo!("Nss implementation for Aead using AES_GCM");
///     }
///
/// }
/// ```
pub trait AeadAlgorithm {
    const KEY_LEN: usize;
    const TAG_LEN: usize;
    const NONCE_LEN: usize;
}

/// Represents an AES_128_GCM Aead algorithm as described
/// in [RFC 5516, section 5.1](https://datatracker.ietf.org/doc/html/rfc5116#section-5.1)
pub struct Aes128Gcm {}

impl AeadAlgorithm for Aes128Gcm {
    const KEY_LEN: usize = 16;
    const NONCE_LEN: usize = 96 / 8;
    const TAG_LEN: usize = 16;
}

/// Represents an AES_256_GCM Aead algorithm as described
/// in [RFC 5516, section 5.1](https://datatracker.ietf.org/doc/html/rfc5116#section-5.2)
pub struct Aes256Gcm {}

impl AeadAlgorithm for Aes256Gcm {
    const KEY_LEN: usize = 32;
    const NONCE_LEN: usize = 96 / 8;
    const TAG_LEN: usize = 16;
}

/// AES-256 in CBC mode with HMAC-SHA256 tags and 128 bit nonces.
/// This is a Sync 1.5 specific encryption scheme, do not use for new
/// applications, there are better options out there nowadays.
/// Important note: The HMAC tag verification should be done against the
/// base64 representation of the ciphertext.
/// More details here: https://mozilla-services.readthedocs.io/en/latest/sync/storageformat5.html#record-encryption
pub struct SyncAes256CBC {}

impl AeadAlgorithm for SyncAes256CBC {
    const KEY_LEN: usize = 64;
    const NONCE_LEN: usize = 128 / 8;
    const TAG_LEN: usize = 32;
}

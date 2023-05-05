/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use thiserror::Error;

pub(crate) type Result<T> = std::result::Result<T, JwCryptoError>;

#[derive(Error, Debug)]
pub enum JwCryptoError {
    #[error("Deserialization error")]
    DeserializationError,
    #[error("Illegal state error: {0}")]
    IllegalState(&'static str),
    #[error("Partial implementation error: {0}")]
    PartialImplementation(&'static str),
    #[error("Base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("Crypto error: {0}")]
    CryptoError(#[from] rc_crypto::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("UTF8 decode error: {0}")]
    UTF8DecodeError(#[from] std::string::FromUtf8Error),
    #[error("InvalidKey")]
    InvalidKey,
    #[error("EmptyCyphertext")]
    EmptyCyphertext,
}

/// Error for the EncryptorDecryptor struct
///
/// The Display implementation will include a description of what was being encrypted/decrypted
/// when the error happened
///
/// This has a nice interaction with `error_support::ErrorHandling`. Crates will typically define
/// an internal error with a variant that wraps `EncryptorDecryptorError`.  As long as the Display
/// implementation for that variant includes the `EncryptorDecryptorError` string, then the
/// description will be included in error reports to Sentry and other systems.
#[derive(Error, Debug)]
#[error("{from} ({description})")]
pub struct EncryptorDecryptorError {
    // Underlying error
    pub from: JwCryptoError,
    pub description: String,
}

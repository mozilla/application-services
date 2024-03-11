//! Error exposed from all cryptographic operations.
//!
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Error during AEAD operation: {0}")]
    AeadError(String),
    #[error("Error during digest: {0}")]
    DigestError(String),
    #[error("Error duing HKDF operation: {0}")]
    HkdfError(String),
    #[error("Error duing Hmac operation: {0}")]
    HmacError(String),
    #[error("Error during Rand operation: {0}")]
    RandError(String),
    #[error("Attempting to get a cryptographer, but one was not set")]
    CryptographerNotSet,
    #[error("Unable to set cryptographer")]
    UnableToSetCryptographer,
    #[error("Error during Agreement operation: {0}")]
    AgreementError(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

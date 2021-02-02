/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

impl From<Error> for ffi_support::ExternError {
    fn from(e: Error) -> ffi_support::ExternError {
        ffi_support::ExternError::new_error(e.kind().error_code(), format!("{:?}", e))
    }
}

error_support::define_error! {
    ErrorKind {
        (StorageSqlError, rusqlite::Error),
        (UrlParseError, url::ParseError),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    /// An unspecified general error has occured
    #[error("General Error: {0:?}")]
    GeneralError(String),

    #[error("Crypto error: {0}")]
    CryptoError(String),

    /// A Client communication error
    #[error("Communication Error: {0:?}")]
    CommunicationError(String),

    /// An error returned from the registration Server
    #[error("Communication Server Error: {0:?}")]
    CommunicationServerError(String),

    /// Channel is already registered, generate new channelID
    #[error("Channel already registered.")]
    AlreadyRegisteredError,

    /// An error with Storage
    #[error("Storage Error: {0:?}")]
    StorageError(String),

    #[error("No record for uaid:chid {0:?}:{1:?}")]
    RecordNotFoundError(String, String),

    /// A failure to encode data to/from storage.
    #[error("Error executing SQL: {0}")]
    StorageSqlError(#[from] rusqlite::Error),

    #[error("Missing Registration Token")]
    MissingRegistrationTokenError,

    #[error("Transcoding Error: {0}")]
    TranscodingError(String),

    /// A failure to parse a URL.
    #[error("URL parse error: {0:?}")]
    UrlParseError(#[from] url::ParseError),
}

// Note, be sure to duplicate errors in the Kotlin side
// see RustError.kt
impl ErrorKind {
    pub fn error_code(&self) -> ffi_support::ErrorCode {
        let code = match self {
            ErrorKind::GeneralError(_) => 22,
            ErrorKind::CryptoError(_) => 24,
            ErrorKind::CommunicationError(_) => 25,
            ErrorKind::CommunicationServerError(_) => 26,
            ErrorKind::AlreadyRegisteredError => 27,
            ErrorKind::StorageError(_) => 28,
            ErrorKind::StorageSqlError(_) => 29,
            ErrorKind::MissingRegistrationTokenError => 30,
            ErrorKind::TranscodingError(_) => 31,
            ErrorKind::RecordNotFoundError(_, _) => 32,
            ErrorKind::UrlParseError(_) => 33,
        };
        ffi_support::ErrorCode::new(code)
    }
}

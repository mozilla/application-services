/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt;
use std::result;

use failure::{Backtrace, Context, Fail};
use ffi_support;

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub struct Error(Box<Context<ErrorKind>>);

impl Fail for Error {
    #[inline]
    fn cause(&self) -> Option<&dyn Fail> {
        self.0.cause()
    }

    #[inline]
    fn backtrace(&self) -> Option<&Backtrace> {
        self.0.backtrace()
    }
}

impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&*self.0, f)
    }
}

impl Error {
    #[inline]
    pub fn kind(&self) -> &ErrorKind {
        &*self.0.get_context()
    }

    pub fn internal(msg: &str) -> Self {
        ErrorKind::InternalError(msg.to_owned()).into()
    }
}

impl From<ErrorKind> for Error {
    #[inline]
    fn from(kind: ErrorKind) -> Error {
        Error(Box::new(Context::new(kind)))
    }
}

impl From<Context<ErrorKind>> for Error {
    #[inline]
    fn from(inner: Context<ErrorKind>) -> Error {
        Error(Box::new(inner))
    }
}

impl From<openssl::error::ErrorStack> for Error {
    #[inline]
    fn from(inner: openssl::error::ErrorStack) -> Error {
        Error(Box::new(Context::new(ErrorKind::OpenSSLError(format!(
            "{:?}",
            inner
        )))))
    }
}

impl From<Error> for ffi_support::ExternError {
    fn from(e: Error) -> ffi_support::ExternError {
        ffi_support::ExternError::new_error(e.kind().error_code(), format!("{:?}", e))
    }
}

macro_rules! impl_from_error {
    ($(($variant:ident, $type:ty)),+) => ($(
        impl From<$type> for ErrorKind {
            #[inline]
            fn from(e: $type) -> ErrorKind {
                ErrorKind::$variant(e)
            }
        }

        impl From<$type> for Error {
            #[inline]
            fn from(e: $type) -> Error {
                ErrorKind::from(e).into()
            }
        }
    )*);
}

impl_from_error! {
    (StorageSqlError, rusqlite::Error),
    (UrlParseError, url::ParseError)
}

#[derive(Debug, Fail)]
pub enum ErrorKind {
    /// An unspecified general error has occured
    #[fail(display = "General Error: {:?}", _0)]
    GeneralError(String),

    /// An unspecifed Internal processing error has occurred
    #[fail(display = "Internal Error: {:?}", _0)]
    InternalError(String),

    /// An unknown OpenSSL Cryptography error
    #[fail(display = "OpenSSL Error: {:?}", _0)]
    OpenSSLError(String),

    /// A Client communication error
    #[fail(display = "Communication Error: {:?}", _0)]
    CommunicationError(String),

    /// An error returned from the registration Server
    #[fail(display = "Communication Server Error: {:?}", _0)]
    CommunicationServerError(String),

    /// Channel is already registered, generate new channelID
    #[fail(display = "Channel already registered.")]
    AlreadyRegisteredError,

    /// An error with Storage
    #[fail(display = "Storage Error: {:?}", _0)]
    StorageError(String),

    /// A failure to encode data to/from storage.
    #[fail(display = "Error executing SQL: {}", _0)]
    StorageSqlError(#[fail(cause)] rusqlite::Error),

    #[fail(display = "Missing Registration Token")]
    MissingRegistrationTokenError,

    #[fail(display = "Transcoding Error: {}", _0)]
    TranscodingError(String),

    #[fail(display = "Encryption Error: {}", _0)]
    EncryptionError(String),

    /// A failure to parse a URL.
    #[fail(display = "URL parse error: {:?}", _0)]
    UrlParseError(#[fail(cause)] url::ParseError),
}

// Note, be sure to duplicate errors in the Kotlin side
// see RustError.kt
impl ErrorKind {
    pub fn error_code(&self) -> ffi_support::ErrorCode {
        let code = match self {
            ErrorKind::GeneralError(_) => 22,
            ErrorKind::InternalError(_) => 23,
            ErrorKind::OpenSSLError(_) => 24,
            ErrorKind::CommunicationError(_) => 25,
            ErrorKind::CommunicationServerError(_) => 26,
            ErrorKind::AlreadyRegisteredError => 27,
            ErrorKind::StorageError(_) => 28,
            ErrorKind::StorageSqlError(_) => 29,
            ErrorKind::MissingRegistrationTokenError => 30,
            ErrorKind::TranscodingError(_) => 31,
            ErrorKind::EncryptionError(_) => 32,
            ErrorKind::UrlParseError(_) => 33,
        };
        ffi_support::ErrorCode::new(code)
    }
}

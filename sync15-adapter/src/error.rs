/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::time::SystemTime;
use reqwest::{self, StatusCode as HttpStatusCode};
use failure::{self, Fail, Context, Backtrace, SyncFailure};
use std::{fmt, result, string};
use std::boxed::Box;
use openssl;
use base64;
use serde_json;
use hawk;

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub struct Error(Box<Context<ErrorKind>>);

impl Fail for Error {
    #[inline]
    fn cause(&self) -> Option<&Fail> {
        self.0.cause()
    }

    #[inline]
    fn backtrace(&self) -> Option<&Backtrace> {
        self.0.backtrace()
    }
}

impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&*self.0, f)
    }
}

impl Error {
    #[inline]
    pub fn kind(&self) -> &ErrorKind {
        &*self.0.get_context()
    }

    pub fn is_not_found(&self) -> bool {
        match self.kind() {
            ErrorKind::StorageHttpError { code: HttpStatusCode::NotFound, .. } => true,
            _ => false
        }
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

#[derive(Debug, Fail)]
pub enum ErrorKind {
    #[fail(display = "Key {} had wrong length, got {}, expected {}", _0, _1, _2)]
    BadKeyLength(&'static str, usize, usize),

    #[fail(display = "SHA256 HMAC Mismatch error")]
    HmacMismatch,

    // TODO: it would be nice if this were _0.to_u16(), but we cant have an expression there...
    #[fail(display = "HTTP status {} when requesting a token from the tokenserver", _0)]
    TokenserverHttpError(HttpStatusCode),

    #[fail(display = "HTTP status {} during a storage request to \"{}\"", code, route)]
    StorageHttpError { code: HttpStatusCode, route: String },

    #[fail(display = "Server requested backoff. Retry after {:?}", _0)]
    BackoffError(SystemTime),

    #[fail(display = "No meta/global record is present on the server")]
    NoMetaGlobal,

    #[fail(display = "Have not fetched crypto/keys yet, or the keys are not present")]
    NoCryptoKeys,

    #[fail(display = "Outgoing record is too large to upload")]
    RecordTooLargeError,

    #[fail(display = "The batch was not committed due to being interrupted")]
    BatchInterrupted,

    // Do we want to record the concrete problems?
    #[fail(display = "Not all records were successfully uploaded")]
    RecordUploadFailed,

    /// Used for things like a node reassignment or an unexpected syncId
    /// implying the app needs to "reset" its understanding of remote storage.
    #[fail(display = "The server has reset the storage for this account")]
    StorageResetError,

    #[fail(display = "Unacceptable URL: {}", _0)]
    UnacceptableUrl(String),

    #[fail(display = "Missing server timestamp header in request")]
    MissingServerTimestamp,

    #[fail(display = "Unexpected server behavior during batch upload: {}", _0)]
    ServerBatchProblem(&'static str),

    #[fail(display = "Error reported by storage: {}", _0)]
    StoreError(#[fail(cause)] failure::Error),

    #[fail(display = "Setup state machine cycle detected")]
    SetupStateCycleError,

    #[fail(display = "Client upgrade required; server storage version too new")]
    ClientUpgradeRequired,

    #[fail(display = "Unexpected state in setup state machine")]
    UnexpectedSetupState,

    // Basically reimplement error_chain's foreign_links. (Ugh, this sucks)

    #[fail(display = "OpenSSL error: {}", _0)]
    OpensslError(#[fail(cause)] openssl::error::ErrorStack),

    #[fail(display = "Base64 decode error: {}", _0)]
    Base64Decode(#[fail(cause)] base64::DecodeError),

    #[fail(display = "JSON parse error: {}", _0)]
    JsonError(#[fail(cause)] serde_json::Error),

    #[fail(display = "Bad cleartext UTF8: {}", _0)]
    BadCleartextUtf8(#[fail(cause)] string::FromUtf8Error),

    #[fail(display = "Network error: {}", _0)]
    RequestError(#[fail(cause)] reqwest::Error),

    #[fail(display = "HAWK error: {}", _0)]
    HawkError(#[fail(cause)] SyncFailure<hawk::Error>),

    #[fail(display = "Malformed URL error: {}", _0)]
    MalformedUrl(#[fail(cause)] reqwest::UrlError),
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
    (OpensslError, ::openssl::error::ErrorStack),
    (Base64Decode, ::base64::DecodeError),
    (JsonError, ::serde_json::Error),
    (BadCleartextUtf8, ::std::string::FromUtf8Error),
    (RequestError, ::reqwest::Error),
    (MalformedUrl, ::reqwest::UrlError)
}

// ::hawk::Error uses error_chain, and so it's not trivially compatible with failure.
// We have to box it inside a SyncError (which allows errors to be accessed from multiple
// threads at the same time, which failure requires for some reason...).
impl From<hawk::Error> for ErrorKind {
    #[inline]
    fn from(e: hawk::Error) -> ErrorKind {
        ErrorKind::HawkError(SyncFailure::new(e))
    }
}
impl From<hawk::Error> for Error {
    #[inline]
    fn from(e: hawk::Error) -> Error {
        ErrorKind::from(e).into()
    }
}

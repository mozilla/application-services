/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use failure::{self, Backtrace, Context, Fail, SyncFailure};
use std::boxed::Box;
use std::time::SystemTime;
use std::{fmt, result, string};

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
            ErrorKind::StorageHttpError { code: 404, .. } => true,
            _ => false,
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

    #[fail(
        display = "HTTP status {} when requesting a token from the tokenserver",
        _0
    )]
    TokenserverHttpError(u16),

    #[fail(
        display = "HTTP status {} during a storage request to \"{}\"",
        code, route
    )]
    StorageHttpError { code: u16, route: String },

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

    #[fail(display = "Setup state machine cycle detected")]
    SetupStateCycleError,

    #[fail(display = "Client upgrade required; server storage version too new")]
    ClientUpgradeRequired,

    #[fail(display = "Setup state machine disallowed state {}", _0)]
    DisallowedStateError(&'static str),

    #[fail(display = "Store error: {}", _0)]
    StoreError(#[fail(cause)] failure::Error),

    // Basically reimplement error_chain's foreign_links. (Ugh, this sucks)
    #[fail(display = "OpenSSL error: {}", _0)]
    OpensslError(#[fail(cause)] openssl::error::ErrorStack),

    #[fail(display = "Base64 decode error: {}", _0)]
    Base64Decode(#[fail(cause)] base64::DecodeError),

    #[fail(display = "JSON error: {}", _0)]
    JsonError(#[fail(cause)] serde_json::Error),

    #[fail(display = "Bad cleartext UTF8: {}", _0)]
    BadCleartextUtf8(#[fail(cause)] string::FromUtf8Error),

    #[fail(display = "Network error: {}", _0)]
    RequestError(#[fail(cause)] viaduct::Error),

    #[fail(display = "Unexpected HTTP status: {}", _0)]
    UnexpectedStatus(#[fail(cause)] viaduct::UnexpectedStatus),

    #[fail(display = "HAWK error: {}", _0)]
    HawkError(#[fail(cause)] SyncFailure<hawk::Error>),

    #[fail(display = "URL parse error: {}", _0)]
    MalformedUrl(#[fail(cause)] url::ParseError),
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
    (RequestError, viaduct::Error),
    (UnexpectedStatus, viaduct::UnexpectedStatus),
    (MalformedUrl, url::ParseError),
    // A bit dubious, since we only want this to happen inside `synchronize`
    (StoreError, ::failure::Error)
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

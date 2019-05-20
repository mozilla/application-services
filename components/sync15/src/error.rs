/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use failure::{Fail, SyncFailure};
use interrupt::Interrupted;
use std::string;
use std::time::SystemTime;

#[derive(Debug)]
pub enum StorageHttpError {
    NotFound { route: String },
    // 401
    Unauthorized { route: String },
    // 412
    PreconditionFailed { route: String },
    // 5XX
    ServerError { route: String, status: u16 }, // TODO: info for "retry-after" and backoff handling etc here.
    // Other HTTP responses.
    RequestFailed { route: String, status: u16 },
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

    #[fail(display = "HTTP storage error: {:?}", _0)]
    StorageHttpError(StorageHttpError),

    #[fail(display = "Server requested backoff. Retry after {:?}", _0)]
    BackoffError(SystemTime),

    #[fail(display = "Outgoing record is too large to upload")]
    RecordTooLargeError,

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

    #[fail(
        display = "It appears some other client is also trying to setup storage; try again later"
    )]
    SetupRace,

    #[fail(display = "Client upgrade required; server storage version too new")]
    ClientUpgradeRequired,

    // This means that our global state machine needs to enter a state (such as
    // "FreshStartNeeded", but the allowed_states don't include that state.)
    // It typically means we are trying to do a "fast" or "read-only" sync.
    #[fail(display = "Our storage needs setting up and we can't currently do it")]
    SetupRequired,

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

    #[fail(display = "The operation was interrupted.")]
    Interrupted(#[fail(cause)] Interrupted),
}

error_support::define_error! {
    ErrorKind {
        (OpensslError, openssl::error::ErrorStack),
        (Base64Decode, base64::DecodeError),
        (JsonError, serde_json::Error),
        (BadCleartextUtf8, std::string::FromUtf8Error),
        (RequestError, viaduct::Error),
        (UnexpectedStatus, viaduct::UnexpectedStatus),
        (MalformedUrl, url::ParseError),
        // A bit dubious, since we only want this to happen inside `synchronize`
        (StoreError, failure::Error),
        (Interrupted, Interrupted),
    }
}

// These can got away when we update to the next version of Hawk,
// in https://github.com/mozilla/application-services/pull/1050
impl From<hawk::Error> for ErrorKind {
    #[cold]
    fn from(e: hawk::Error) -> ErrorKind {
        ErrorKind::HawkError(SyncFailure::new(e))
    }
}

impl From<hawk::Error> for Error {
    #[cold]
    fn from(e: hawk::Error) -> Self {
        ErrorKind::from(e).into()
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use interrupt_support::Interrupted;
use std::time::SystemTime;
use sync15_traits::{request::UnacceptableBaseUrl, SyncTraitsError};
/// This enum is to discriminate `StorageHttpError`, and not used as an error.
#[derive(Debug, Clone)]
pub enum ErrorResponse {
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

#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    // These are errors duplicated from SyncTraitsError, so that consumers can
    // deal with errors from just 1 of the crates and not care which one of
    // then actually caused the error.
    #[error("Key {0} had wrong length, got {1}, expected {2}")]
    BadKeyLength(&'static str, usize, usize),

    #[error("SHA256 HMAC Mismatch error")]
    HmacMismatch,

    #[error("Crypto/NSS error: {0}")]
    CryptoError(#[from] rc_crypto::Error),

    #[error("Base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Bad cleartext UTF8: {0}")]
    BadCleartextUtf8(#[from] std::string::FromUtf8Error),

    #[error("HAWK error: {0}")]
    HawkError(#[from] rc_crypto::hawk::Error),

    //
    // Errors specific to this module.
    //
    #[error("HTTP status {0} when requesting a token from the tokenserver")]
    TokenserverHttpError(u16),

    #[error("HTTP storage error: {0:?}")]
    StorageHttpError(ErrorResponse),

    #[error("Server requested backoff. Retry after {0:?}")]
    BackoffError(SystemTime),

    #[error("Outgoing record is too large to upload")]
    RecordTooLargeError,

    // Do we want to record the concrete problems?
    #[error("Not all records were successfully uploaded")]
    RecordUploadFailed,

    /// Used for things like a node reassignment or an unexpected syncId
    /// implying the app needs to "reset" its understanding of remote storage.
    #[error("The server has reset the storage for this account")]
    StorageResetError,

    #[error("Unacceptable URL: {0}")]
    UnacceptableUrl(String),

    #[error("Missing server timestamp header in request")]
    MissingServerTimestamp,

    #[error("Unexpected server behavior during batch upload: {0}")]
    ServerBatchProblem(&'static str),

    #[error("It appears some other client is also trying to setup storage; try again later")]
    SetupRace,

    #[error("Client upgrade required; server storage version too new")]
    ClientUpgradeRequired,

    // This means that our global state machine needs to enter a state (such as
    // "FreshStartNeeded", but the allowed_states don't include that state.)
    // It typically means we are trying to do a "fast" or "read-only" sync.
    #[error("Our storage needs setting up and we can't currently do it")]
    SetupRequired,

    #[error("Store error: {0}")]
    StoreError(#[from] anyhow::Error),

    #[error("Network error: {0}")]
    RequestError(#[from] viaduct::Error),

    #[error("Unexpected HTTP status: {0}")]
    UnexpectedStatus(#[from] viaduct::UnexpectedStatus),

    #[error("URL parse error: {0}")]
    MalformedUrl(#[from] url::ParseError),

    #[error("The operation was interrupted.")]
    Interrupted(#[from] Interrupted),
}

error_support::define_error! {
    ErrorKind {
        (JsonError, serde_json::Error),
        (RequestError, viaduct::Error),
        (UnexpectedStatus, viaduct::UnexpectedStatus),
        (MalformedUrl, url::ParseError),
        // A bit dubious, since we only want this to happen inside `synchronize`
        (StoreError, anyhow::Error),
        (Interrupted, Interrupted),
        (HawkError, rc_crypto::hawk::Error),
    }
}

impl From<SyncTraitsError> for ErrorKind {
    fn from(e: SyncTraitsError) -> ErrorKind {
        match e {
            SyncTraitsError::BadKeyLength(key, got, expected) => {
                ErrorKind::BadKeyLength(key, got, expected)
            }
            SyncTraitsError::HmacMismatch => ErrorKind::HmacMismatch,
            SyncTraitsError::CryptoError(e) => ErrorKind::CryptoError(e),
            SyncTraitsError::Base64Decode(e) => ErrorKind::Base64Decode(e),
            SyncTraitsError::JsonError(e) => ErrorKind::JsonError(e),
            SyncTraitsError::BadCleartextUtf8(e) => ErrorKind::BadCleartextUtf8(e),
            SyncTraitsError::HawkError(e) => ErrorKind::HawkError(e),
        }
    }
}

impl From<SyncTraitsError> for Error {
    fn from(e: SyncTraitsError) -> Self {
        Error::from(ErrorKind::from(e))
    }
}

// XXX - we should just move this `UnacceptableBaseUrl` into the `SyncTraitsError` enum.
impl From<UnacceptableBaseUrl> for ErrorKind {
    fn from(e: UnacceptableBaseUrl) -> ErrorKind {
        ErrorKind::UnacceptableUrl(e.to_string())
    }
}

impl From<UnacceptableBaseUrl> for Error {
    fn from(e: UnacceptableBaseUrl) -> Self {
        Error::from(ErrorKind::from(e))
    }
}

impl Error {
    pub(crate) fn get_backoff(&self) -> Option<SystemTime> {
        if let ErrorKind::BackoffError(time) = self.kind() {
            Some(*time)
        } else {
            None
        }
    }
}

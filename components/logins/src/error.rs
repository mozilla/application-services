/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::ffi::OsString;
pub type Result<T> = std::result::Result<T, LoginsError>;
// Functions which are part of the public API should use this Result.
pub type ApiResult<T> = std::result::Result<T, LoginsStorageError>;

pub use error_support::handle_error;
use error_support::{ErrorHandling, GetErrorHandling};
use sync15::ErrorKind as Sync15ErrorKind;

// Errors we return via the public interface.
//
// Named `LoginsStorageError` for backwards compatibility reasons, although
// this name shouldn't need to be used anywhere other than this file and the .udl
//
// Note that there is no `Into` between public and internal errors, but
// instead the `ErrorHandling` mechanisms are used to explicitly convert
// when necessary.
//
// XXX - not clear that these actually need to use `thiserror`? Certainly
// not necessary to use `#[from]` though.
#[derive(Debug, thiserror::Error)]
pub enum LoginsStorageError {
    #[error("Invalid login: {0}")]
    InvalidRecord(String),

    #[error("No record with guid exists (when one was required): {0:?}")]
    NoSuchRecord(String),

    #[error("Encryption key is in the correct format, but is not the correct key.")]
    IncorrectKey,

    #[error("{0}")]
    Interrupted(String),

    #[error("SyncAuthInvalid error {0}")]
    SyncAuthInvalid(String),

    // This error is emitted if a request to a sync server failed.
    /// We can probably kill this? The sync manager is what cares about this.
    #[error("RequestFailed error: {0}")]
    RequestFailed(String),

    #[error("Unexpected Error: {0}")]
    UnexpectedLoginsStorageError(String),
}

/// Logins error type
/// These are "internal" errors used by the implementation. This error type
/// is never returned to the consumer.
#[derive(Debug, thiserror::Error)]
pub enum LoginsError {
    #[error("Invalid login: {0}")]
    InvalidLogin(#[from] InvalidLogin),

    #[error("The `sync_status` column in DB has an illegal value: {0}")]
    BadSyncStatus(u8),

    #[error("No record with guid exists (when one was required): {0:?}")]
    NoSuchRecord(String),

    // Fennec import only works on empty logins tables.
    #[error("The logins tables are not empty")]
    NonEmptyTable,

    #[error("local encryption key not set")]
    EncryptionKeyMissing,

    #[error("Error synchronizing: {0}")]
    SyncAdapterError(#[from] sync15::Error),

    #[error("Error parsing JSON data: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Error executing SQL: {0}")]
    SqlError(#[from] rusqlite::Error),

    #[error("Error parsing URL: {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("Invalid path: {0:?}")]
    InvalidPath(OsString),

    #[error("Invalid database file: {0}")]
    InvalidDatabaseFile(String),

    #[error("Invalid encryption key")]
    InvalidKey,

    #[error("Crypto Error: {0}")]
    CryptoError(#[from] jwcrypto::JwCryptoError),

    #[error("{0}")]
    Interrupted(#[from] interrupt_support::Interrupted),

    #[error("IOError: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Migration Error: {0}")]
    MigrationError(String),
}

/// Error::InvalidLogin subtypes
#[derive(Debug, thiserror::Error)]
pub enum InvalidLogin {
    // EmptyOrigin error occurs when the login's origin field is empty.
    #[error("Origin is empty")]
    EmptyOrigin,
    #[error("Password is empty")]
    EmptyPassword,
    #[error("Login already exists")]
    DuplicateLogin,
    #[error("Both `formActionOrigin` and `httpRealm` are present")]
    BothTargets,
    #[error("Neither `formActionOrigin` or `httpRealm` are present")]
    NoTarget,
    #[error("Login has illegal field: {field_info}")]
    IllegalFieldValue { field_info: String },
}

// Define how our internal errors are handled and converted to external errors.
impl GetErrorHandling for LoginsError {
    type ExternalError = LoginsStorageError;

    // Return how to handle our internal errors
    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        // WARNING: The details inside the `LoginsStorageError` we return should not
        // contain any personally identifying information.
        // However, because many of the string details come from the underlying
        // internal error, we operate on a best-effort basis, since we can't be
        // completely sure that our dependencies don't leak PII in their error
        // strings.  For example, `rusqlite::Error` could include data from a
        // user's database in their errors, which would then cause it to appear
        // in our `LoginsStorageError::Unexpected` structs, log messages, etc.
        // But because we've never seen that in practice we are comfortable
        // forwarding that error message into ours without attempting to sanitize.
        match self {
            Self::InvalidLogin(why) => {
                ErrorHandling::passthrough(LoginsStorageError::InvalidRecord(why.to_string()))
            }
            // Our internal "no such record" error is converted to our public "no such record" error, with no logging and no error reporting.
            Self::NoSuchRecord(guid) => {
                ErrorHandling::passthrough(LoginsStorageError::NoSuchRecord(guid.to_string()))
            }
            // NonEmptyTable error is just a sanity check to ensure we aren't asked to migrate into an
            // existing DB - consumers should never actually do this, and will never expect to handle this as a specific
            // error - so it gets reported to the error reporter and converted to an "internal" error.
            Self::NonEmptyTable => ErrorHandling::unexpected(
                LoginsStorageError::UnexpectedLoginsStorageError(
                    "must be an empty DB to migrate".to_string(),
                ),
                Some("migration"),
            ),
            Self::CryptoError(_) => {
                ErrorHandling::log(LoginsStorageError::IncorrectKey, log::Level::Warn)
            }
            Self::Interrupted(_) => {
                ErrorHandling::passthrough(LoginsStorageError::Interrupted(self.to_string()))
            }
            Self::SyncAdapterError(e) => match e.kind() {
                Sync15ErrorKind::TokenserverHttpError(401) | Sync15ErrorKind::BadKeyLength(..) => {
                    ErrorHandling::log(
                        LoginsStorageError::SyncAuthInvalid(e.to_string()),
                        log::Level::Warn,
                    )
                }
                Sync15ErrorKind::RequestError(_) => ErrorHandling::log(
                    LoginsStorageError::RequestFailed(e.to_string()),
                    log::Level::Warn,
                ),
                _ => ErrorHandling::unexpected(
                    LoginsStorageError::UnexpectedLoginsStorageError(self.to_string()),
                    Some("sync"),
                ),
            },
            // This list is partial - not clear if a best-practice should be to ask that every
            // internal error is listed here (and remove this default branch) to ensure every error
            // is considered, or whether this default is fine for obscure errors?
            // But it's fine for now because errors were always converted with a default
            // branch to "unexpected"
            _ => ErrorHandling::unexpected(
                LoginsStorageError::UnexpectedLoginsStorageError(self.to_string()),
                None,
            ),
        }
    }
}

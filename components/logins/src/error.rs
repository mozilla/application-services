/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::ffi::OsString;
pub type Result<T> = std::result::Result<T, Error>;
// Functions which are part of the public API should use this Result.
pub type ApiResult<T> = std::result::Result<T, LoginsApiError>;

pub use error_support::{breadcrumb, handle_error, report_error};
use error_support::{ErrorHandling, GetErrorHandling};
use sync15::Error as Sync15Error;

// Errors we return via the public interface.
#[derive(Debug, thiserror::Error)]
pub enum LoginsApiError {
    #[error("Invalid login: {reason}")]
    InvalidRecord { reason: String },

    #[error("No record with guid exists (when one was required): {reason:?}")]
    NoSuchRecord { reason: String },

    #[error("Encryption key is in the correct format, but is not the correct key.")]
    IncorrectKey,

    #[error("{reason}")]
    Interrupted { reason: String },

    #[error("SyncAuthInvalid error {reason}")]
    SyncAuthInvalid { reason: String },

    #[error("Unexpected Error: {reason}")]
    UnexpectedLoginsApiError { reason: String },
}

/// Logins error type
/// These are "internal" errors used by the implementation. This error type
/// is never returned to the consumer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Malformed incoming record")]
    MalformedIncomingRecord,

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
    // Login has an illegal origin field, split off from IllegalFieldValue since this is a known
    // issue with the Desktop logins and we don't want to report it to Sentry (see #5233).
    #[error("Login has illegal origin")]
    IllegalOrigin,
    #[error("Login has illegal field: {field_info}")]
    IllegalFieldValue { field_info: String },
}

// Define how our internal errors are handled and converted to external errors
// See `support/error/README.md` for how this works, especially the warning about PII.
impl GetErrorHandling for Error {
    type ExternalError = LoginsApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        match self {
            Self::InvalidLogin(why) => ErrorHandling::convert(LoginsApiError::InvalidRecord {
                reason: why.to_string(),
            }),
            Self::MalformedIncomingRecord => {
                ErrorHandling::convert(LoginsApiError::InvalidRecord {
                    reason: "invalid incoming record".to_string(),
                })
            }
            // Our internal "no such record" error is converted to our public "no such record" error, with no logging and no error reporting.
            Self::NoSuchRecord(guid) => ErrorHandling::convert(LoginsApiError::NoSuchRecord {
                reason: guid.to_string(),
            }),
            // NonEmptyTable error is just a sanity check to ensure we aren't asked to migrate into an
            // existing DB - consumers should never actually do this, and will never expect to handle this as a specific
            // error - so it gets reported to the error reporter and converted to an "internal" error.
            Self::NonEmptyTable => {
                ErrorHandling::convert(LoginsApiError::UnexpectedLoginsApiError {
                    reason: "must be an empty DB to migrate".to_string(),
                })
                .report_error("logins-migration")
            }
            Self::CryptoError(_) => {
                ErrorHandling::convert(LoginsApiError::IncorrectKey).log_warning()
            }
            Self::Interrupted(_) => ErrorHandling::convert(LoginsApiError::Interrupted {
                reason: self.to_string(),
            }),
            Self::SyncAdapterError(e) => match e {
                Sync15Error::TokenserverHttpError(401) | Sync15Error::BadKeyLength(..) => {
                    ErrorHandling::convert(LoginsApiError::SyncAuthInvalid {
                        reason: e.to_string(),
                    })
                    .log_warning()
                }
                Sync15Error::RequestError(_) => {
                    ErrorHandling::convert(LoginsApiError::UnexpectedLoginsApiError {
                        reason: e.to_string(),
                    })
                    .log_warning()
                }
                _ => ErrorHandling::convert(LoginsApiError::UnexpectedLoginsApiError {
                    reason: self.to_string(),
                })
                .report_error("logins-sync"),
            },
            // Errors that are unexpected in the sense that they are too rare to deserve a
            // LoginsApiError variant, but we still don't need to report them to Sentry.
            //
            // For now, just log a warning.  Eventually, it would be nice to count these with
            // telemetry.
            Self::InvalidDatabaseFile(_) => {
                ErrorHandling::convert(LoginsApiError::UnexpectedLoginsApiError {
                    reason: self.to_string(),
                })
                .log_warning()
            }
            // Unexpected errors that we report to Sentry.  We should watch the reports for these
            // and do one or more of these things if we see them:
            //   - Fix the underlying issue
            //   - Add breadcrumbs or other context to help uncover the issue
            //   - Decide that these are expected errors and move them to the above case
            _ => ErrorHandling::convert(LoginsApiError::UnexpectedLoginsApiError {
                reason: self.to_string(),
            })
            .report_error("logins-unexpected"),
        }
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error_support::{ErrorHandling, GetErrorHandling};

/// Result enum for the public interface
pub type ApiResult<T> = std::result::Result<T, TabsApiError>;
/// Result enum for internal functions
pub type Result<T> = std::result::Result<T, Error>;

// Errors we return via the public interface.
#[derive(Debug, thiserror::Error)]
pub enum TabsApiError {
    #[error("SyncError: {reason}")]
    SyncError { reason: String },

    #[error("SqlError: {reason}")]
    SqlError { reason: String },

    #[error("Unexpected tabs error: {reason}")]
    UnexpectedTabsError { reason: String },
}

// Error we use internally
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(feature = "full-sync")]
    #[error("Error synchronizing: {0}")]
    SyncAdapterError(#[from] sync15::Error),

    // Note we are abusing this as a kind of "mis-matched feature" error.
    // This works because when `full-sync` isn't enabled we don't actually
    // handle any sync15 errors as the bridged-engine never returns them.
    #[cfg(not(feature = "full-sync"))]
    #[error("Sync feature is disabled: {0}")]
    SyncAdapterError(String),

    #[error("Error parsing JSON data: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Missing SyncUnlockInfo Local ID")]
    MissingLocalIdError,

    #[error("Error parsing URL: {0}")]
    UrlParseError(#[from] url::ParseError),

    #[error("Error executing SQL: {0}")]
    SqlError(#[from] rusqlite::Error),

    #[error("Error opening database: {0}")]
    OpenDatabaseError(#[from] sql_support::open_database::Error),
}

// Define how our internal errors are handled and converted to external errors.
impl GetErrorHandling for Error {
    type ExternalError = TabsApiError;

    // Return how to handle our internal errors
    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        // WARNING: The details inside the `TabsApiError` we return should not contain any
        // personally identifying information. However, because many of the string details come
        // from the underlying internal error, we operate on a best-effort basis, since we can't be
        // completely sure that our dependencies don't leak PII in their error strings.  For
        // example, `rusqlite::Error` could include data from a user's database in their errors,
        // which would then cause it to appear in our `TabsApiError::SqlError` structs, log
        // messages, etc. But because we've never seen that in practice we are comfortable
        // forwarding that error message into ours without attempting to sanitize.
        match self {
            Self::SyncAdapterError(e) => ErrorHandling::convert(TabsApiError::SyncError {
                reason: e.to_string(),
            })
            .report_error("tabs-sync-error"),
            Self::JsonError(e) => ErrorHandling::convert(TabsApiError::UnexpectedTabsError {
                reason: e.to_string(),
            })
            .report_error("tabs-json-error"),
            Self::MissingLocalIdError => {
                ErrorHandling::convert(TabsApiError::UnexpectedTabsError {
                    reason: "MissingLocalId".to_string(),
                })
                .report_error("tabs-missing-local-id-error")
            }
            Self::UrlParseError(e) => ErrorHandling::convert(TabsApiError::UnexpectedTabsError {
                reason: e.to_string(),
            })
            .report_error("tabs-url-parse-error"),
            Self::SqlError(e) => ErrorHandling::convert(TabsApiError::SqlError {
                reason: e.to_string(),
            })
            .report_error("tabs-sql-error"),
            Self::OpenDatabaseError(e) => ErrorHandling::convert(TabsApiError::SqlError {
                reason: e.to_string(),
            })
            .report_error("tabs-open-database-error"),
        }
    }
}

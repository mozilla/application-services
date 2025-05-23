/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use error_support::{ErrorHandling, GetErrorHandling};
use interrupt_support::Interrupted;

/// Result enum for the public API
pub type ApiResult<T> = std::result::Result<T, AutofillApiError>;

/// Result enum for internal functions
pub type Result<T> = std::result::Result<T, Error>;

// Errors we return via the public interface.
#[derive(Debug, thiserror::Error)]
pub enum AutofillApiError {
    #[error("Error executing SQL: {reason}")]
    SqlError { reason: String },

    #[error("Operation interrupted")]
    InterruptedError,

    #[error("Crypto Error: {reason}")]
    CryptoError { reason: String },

    #[error("No record with guid exists: {guid}")]
    NoSuchRecord { guid: String },

    #[error("Unexpected Error: {reason}")]
    UnexpectedAutofillApiError { reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Error opening database: {0}")]
    OpenDatabaseError(#[from] sql_support::open_database::Error),

    #[error("Error executing SQL: {0}")]
    SqlError(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Operation interrupted")]
    InterruptedError(#[from] Interrupted),

    // This will happen if you provide something absurd like
    // "/" or "" as your database path. For more subtley broken paths,
    // we'll likely return an IoError.
    #[error("Illegal database path: {0:?}")]
    IllegalDatabasePath(std::path::PathBuf),

    #[error("JSON Error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Invalid sync payload: {0}")]
    InvalidSyncPayload(String),

    #[error("Crypto Error: {0}")]
    CryptoError(#[from] jwcrypto::JwCryptoError),

    #[error("Missing local encryption key")]
    MissingEncryptionKey,

    #[error("No record with guid exists: {0}")]
    NoSuchRecord(String),
}

// Define how our internal errors are handled and converted to external errors
// See `support/error/README.md` for how this works, especially the warning about PII.
impl GetErrorHandling for Error {
    type ExternalError = AutofillApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        match self {
            Self::OpenDatabaseError(e) => ErrorHandling::convert(AutofillApiError::SqlError {
                reason: e.to_string(),
            })
            .report_error("autofill-open-database-error"),

            Self::SqlError(e) => ErrorHandling::convert(AutofillApiError::SqlError {
                reason: e.to_string(),
            })
            .report_error("autofill-sql-error"),

            Self::IoError(e) => {
                ErrorHandling::convert(AutofillApiError::UnexpectedAutofillApiError {
                    reason: e.to_string(),
                })
                .report_error("autofill-io-error")
            }

            Self::InterruptedError(_) => ErrorHandling::convert(AutofillApiError::InterruptedError),

            Self::IllegalDatabasePath(path) => ErrorHandling::convert(AutofillApiError::SqlError {
                reason: format!("Path not found: {}", path.to_string_lossy()),
            })
            .report_error("autofill-illegal-database-path"),

            Self::JsonError(e) => {
                ErrorHandling::convert(AutofillApiError::UnexpectedAutofillApiError {
                    reason: e.to_string(),
                })
                .report_error("autofill-json-error")
            }

            Self::InvalidSyncPayload(reason) => {
                ErrorHandling::convert(AutofillApiError::UnexpectedAutofillApiError {
                    reason: reason.clone(),
                })
                .report_error("autofill-invalid-sync-payload")
            }

            Self::CryptoError(e) => ErrorHandling::convert(AutofillApiError::CryptoError {
                reason: e.to_string(),
            })
            .report_error("autofill-crypto-error"),

            Self::MissingEncryptionKey => ErrorHandling::convert(AutofillApiError::CryptoError {
                reason: "Missing encryption key".to_string(),
            })
            .report_error("autofill-missing-encryption-key"),

            Self::NoSuchRecord(guid) => {
                ErrorHandling::convert(AutofillApiError::NoSuchRecord { guid: guid.clone() })
                    .log_warning()
            }
        }
    }
}

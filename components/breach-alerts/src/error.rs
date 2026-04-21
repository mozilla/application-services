/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub use error_support::{debug, error, info, trace, warn};
use error_support::{ErrorHandling, GetErrorHandling};

use interrupt_support::Interrupted;

/// Errors returned via the public (FFI) interface.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum BreachAlertsApiError {
    #[error("Unexpected error: {reason}")]
    Unexpected { reason: String },
}

/// Errors used internally.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Error executing SQL: {0}")]
    SqlError(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Operation interrupted")]
    InterruptedError(#[from] Interrupted),

    #[error("Illegal database path: {0:?}")]
    IllegalDatabasePath(std::path::PathBuf),

    #[error("Error opening database: {0}")]
    OpenDatabaseError(#[from] sql_support::open_database::Error),

    #[error("The storage database has been closed")]
    DatabaseConnectionClosed,
}

/// Result for the public API.
pub type ApiResult<T> = std::result::Result<T, BreachAlertsApiError>;

/// Result for internal functions.
pub type Result<T> = std::result::Result<T, Error>;

impl GetErrorHandling for Error {
    type ExternalError = BreachAlertsApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        ErrorHandling::convert(BreachAlertsApiError::Unexpected {
            reason: self.to_string(),
        })
    }
}

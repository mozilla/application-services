/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use error_support::{ErrorHandling, GetErrorHandling};
use remote_settings::RemoteSettingsError;

/// A list of errors that are internal to the component. This is the error
/// type for private and crate-internal methods, and is never returned to the
/// application.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Error opening database: {0}")]
    OpenDatabase(#[from] sql_support::open_database::Error),

    #[error("Error executing SQL: {inner} (context: {context})")]
    Sql {
        inner: rusqlite::Error,
        context: String,
    },

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Error from Remote Settings: {0}")]
    RemoteSettings(#[from] RemoteSettingsError),

    #[error("Remote settings record is missing an attachment (id: u64)")]
    MissingAttachment(String),

    #[error("Operation interrupted")]
    Interrupted(#[from] interrupt_support::Interrupted),

    #[error("SuggestStoreBuilder {0}")]
    SuggestStoreBuilder(String),
}

impl Error {
    fn sql(e: rusqlite::Error, context: impl Into<String>) -> Self {
        Self::Sql {
            inner: e,
            context: context.into(),
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::sql(e, "<none>")
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

impl From<rmp_serde::decode::Error> for Error {
    fn from(e: rmp_serde::decode::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

impl From<rmp_serde::encode::Error> for Error {
    fn from(e: rmp_serde::encode::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

#[extend::ext(name=RusqliteResultExt)]
pub impl<T> Result<T, rusqlite::Error> {
    // Convert an rusqlite::Error to our error type, with a context value
    fn with_context(self, context: &str) -> Result<T, Error> {
        self.map_err(|e| Error::sql(e, context))
    }
}

/// The error type for all Suggest component operations. These errors are
/// exposed to your application, which should handle them as needed.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[non_exhaustive]
pub enum SuggestApiError {
    #[error("Network error: {reason}")]
    Network { reason: String },
    /// The server requested a backoff after too many requests
    #[error("Backoff")]
    Backoff { seconds: u64 },
    /// An operation was interrupted by calling `SuggestStore.interrupt()`
    #[error("Interrupted")]
    Interrupted,
    #[error("Other error: {reason}")]
    Other { reason: String },
}

// Define how our internal errors are handled and converted to external errors
// See `support/error/README.md` for how this works, especially the warning about PII.
impl GetErrorHandling for Error {
    type ExternalError = SuggestApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        match self {
            // Do nothing for interrupted errors, this is just normal operation.
            Self::Interrupted(_) => ErrorHandling::convert(SuggestApiError::Interrupted),
            // Network errors are expected to happen in practice.  Let's log, but not report them.
            Self::RemoteSettings(RemoteSettingsError::Network { reason }) => {
                ErrorHandling::convert(SuggestApiError::Network {
                    reason: reason.clone(),
                })
                .log_warning()
            }
            // Backoff error shouldn't happen in practice, so let's report them for now.
            // If these do happen in practice and we decide that there is a valid reason for them,
            // then consider switching from reporting to Sentry to counting in Glean.
            Self::RemoteSettings(RemoteSettingsError::Backoff { seconds }) => {
                ErrorHandling::convert(SuggestApiError::Backoff { seconds: *seconds })
                    .report_error("suggest-backoff")
            }
            _ => ErrorHandling::convert(SuggestApiError::Other {
                reason: self.to_string(),
            })
            .report_error("suggest-unexpected"),
        }
    }
}

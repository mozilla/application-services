/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error_support::{ErrorHandling, GetErrorHandling};
// Re-export logging helpers.
pub use error_support::{error, trace};

/// Internal convenience wrapper for `std::Result`.
pub type Result<T> = std::result::Result<T, Error>;

/// Public API result type using [`CuratedRecommendationsApiError`], exposed via UniFFI.
pub type ApiResult<T> = std::result::Result<T, CuratedRecommendationsApiError>;

/// Public error type exposed to consumers via UniFFI.
///
/// This is a simplified version of [`Error`] suitable for cross-platform callers,
/// distinguishing network failures from other errors.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CuratedRecommendationsApiError {
    /// A network-level failure (e.g. DNS resolution, connection timeout).
    #[error("Curated recommendations network error: {reason}")]
    Network { reason: String },

    /// Any other error, including HTTP errors and deserialization failures.
    #[error("Curated recommendations error: code {code:?}, reason: {reason}")]
    Other { code: Option<u16>, reason: String },
}

/// Internal error type with fine-grained variants for different failure modes.
///
/// These are converted to [`CuratedRecommendationsApiError`] before being exposed
/// to consumers via the [`GetErrorHandling`] implementation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to parse a URL.
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    /// Failed to send the HTTP request.
    #[error("Error sending request: {0}")]
    Request(#[from] viaduct::ViaductError),

    /// Failed to serialize or deserialize JSON.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The server rejected the request due to invalid input (HTTP 422).
    #[error("Validation error ({code}): {message}")]
    Validation { code: u16, message: String },

    /// The server rejected the request due to malformed syntax (HTTP 400).
    #[error("Bad request ({code}): {message}")]
    BadRequest { code: u16, message: String },

    /// The server encountered an internal error (HTTP 5xx).
    #[error("Server error ({code}): {message}")]
    Server { code: u16, message: String },

    /// An unexpected HTTP status code was received.
    #[error("Unexpected error ({code}): {message}")]
    Unexpected { code: u16, message: String },
}

impl GetErrorHandling for Error {
    type ExternalError = CuratedRecommendationsApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        match self {
            Self::Request { .. } => {
                ErrorHandling::convert(CuratedRecommendationsApiError::Network {
                    reason: self.to_string(),
                })
                .log_warning()
            }

            Self::Validation { code, .. }
            | Self::Server { code, .. }
            | Self::Unexpected { code, .. }
            | Self::BadRequest { code, .. } => {
                ErrorHandling::convert(CuratedRecommendationsApiError::Other {
                    code: Some(*code),
                    reason: self.to_string(),
                })
                .report_error("merino-http-error")
            }

            Self::UrlParse(_) | Self::Json(_) => {
                ErrorHandling::convert(CuratedRecommendationsApiError::Other {
                    code: None,
                    reason: self.to_string(),
                })
                .report_error("merino-unexpected")
            }
        }
    }
}

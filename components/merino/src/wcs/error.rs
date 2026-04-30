/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error_support::{ErrorHandling, GetErrorHandling};

pub type Result<T> = std::result::Result<T, Error>;
pub type ApiResult<T> = std::result::Result<T, WcsApiError>;

/// Public error type exposed to consumers via UniFFI.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum WcsApiError {
    #[error("WCS network error: {reason}")]
    Network { reason: String },

    #[error("WCS error: code {code:?}, reason: {reason}")]
    Other { code: Option<u16>, reason: String },
}

/// Internal error type with fine-grained variants.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("Error sending request: {0}")]
    Request(#[from] viaduct::ViaductError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Validation error ({code}): {message}")]
    Validation { code: u16, message: String },

    #[error("Bad request ({code}): {message}")]
    BadRequest { code: u16, message: String },

    #[error("Server error ({code}): {message}")]
    Server { code: u16, message: String },

    #[error("Unexpected error ({code}): {message}")]
    Unexpected { code: u16, message: String },
}

impl Error {
    /// Returns `true` for transient failures that are worth retrying.
    /// Deterministic errors (4xx, parse failures) return `false`.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Request(_) | Self::Server { .. })
    }
}

impl GetErrorHandling for Error {
    type ExternalError = WcsApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        match self {
            Self::Request { .. } => ErrorHandling::convert(WcsApiError::Network {
                reason: self.to_string(),
            })
            .log_warning(),

            Self::Validation { code, .. }
            | Self::Server { code, .. }
            | Self::Unexpected { code, .. }
            | Self::BadRequest { code, .. } => ErrorHandling::convert(WcsApiError::Other {
                code: Some(*code),
                reason: self.to_string(),
            })
            .report_error("merino-wcs-http-error"),

            Self::UrlParse(_) | Self::Json(_) => ErrorHandling::convert(WcsApiError::Other {
                code: None,
                reason: self.to_string(),
            })
            .report_error("merino-wcs-unexpected"),
        }
    }
}

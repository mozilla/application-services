/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error_support::{ErrorHandling, GetErrorHandling};
// reexport logging helpers.
pub use error_support::{error, trace};

pub type Result<T> = std::result::Result<T, Error>;
pub type ApiResult<T> = std::result::Result<T, CuratedRecommendationsApiError>;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CuratedRecommendationsApiError {
    #[error("Curated recommendations network error: {reason}")]
    Network { reason: String },

    #[error("Curated recommendations error: code {code:?}, reason: {reason}")]
    Other { code: Option<u16>, reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("Error sending request: {0}")]
    Request(#[from] viaduct::Error),

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

            _ => ErrorHandling::convert(CuratedRecommendationsApiError::Other {
                code: None,
                reason: self.to_string(),
            })
            .report_error("merino-unexpected"),
        }
    }
}

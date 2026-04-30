/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub use error_support::error;
use error_support::{ErrorHandling, GetErrorHandling};

pub type Result<T> = std::result::Result<T, Error>;
pub type ApiResult<T> = std::result::Result<T, MerinoWorldCupApiError>;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MerinoWorldCupApiError {
    /// A network-level failure.
    #[error("WorldCup network error: {reason}")]
    Network { reason: String },

    /// Any other error, e.g. HTTP errors, validation errors.
    #[error("WorldCup error: code {code:?}, reason: {reason}")]
    Other { code: Option<u16>, reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to parse a URL.
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    /// Failed to send the HTTP request.
    #[error("Error sending request: {0}")]
    Request(#[from] viaduct::ViaductError),

    /// The server rejected the request due to malformed syntax (HTTP 400).
    #[error("Bad request ({code}): {message}")]
    BadRequest { code: u16, message: String },

    /// The server rejected the request due to invalid input (HTTP 422).
    #[error("Validation error ({code}): {message}")]
    Validation { code: u16, message: String },

    /// The server encountered an internal error (HTTP 5xx).
    #[error("Server error ({code}): {message}")]
    Server { code: u16, message: String },

    /// An unexpected HTTP status code was received.
    #[error("Unexpected error ({code}): {message}")]
    Unexpected { code: u16, message: String },
}

impl GetErrorHandling for Error {
    type ExternalError = MerinoWorldCupApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        match self {
            Self::Request { .. } => ErrorHandling::convert(MerinoWorldCupApiError::Network {
                reason: self.to_string(),
            })
            .log_warning(),

            Self::Validation { code, .. }
            | Self::Server { code, .. }
            | Self::Unexpected { code, .. }
            | Self::BadRequest { code, .. } => {
                ErrorHandling::convert(MerinoWorldCupApiError::Other {
                    code: Some(*code),
                    reason: self.to_string(),
                })
                .report_error("merino-http-error")
            }

            Self::UrlParse(_) => ErrorHandling::convert(MerinoWorldCupApiError::Other {
                code: None,
                reason: self.to_string(),
            })
            .report_error("merino-unexpected"),
        }
    }
}

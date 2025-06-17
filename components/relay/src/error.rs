/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub use error_support::error;
use error_support::{ErrorHandling, GetErrorHandling};

pub type Result<T> = std::result::Result<T, Error>;
pub type ApiResult<T> = std::result::Result<T, RelayApiError>;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum RelayApiError {
    #[error("Relay network error: {reason}")]
    Network { reason: String },

    #[error("Relay API error: {detail}")]
    RelayApi { detail: String },

    #[error("Relay unexpected error: {reason}")]
    Other { reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Relay API error: {0}")]
    RelayApi(String),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP request error: {0}")]
    Viaduct(#[from] viaduct::Error),
    #[error("URL parsing error: {0}")]
    UrlParse(#[from] url::ParseError),
}

impl GetErrorHandling for Error {
    type ExternalError = RelayApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        match self {
            Error::Viaduct(viaduct::Error::NetworkError(e)) => {
                ErrorHandling::convert(RelayApiError::Network {
                    reason: e.to_string(),
                })
                .log_warning()
            }
            Error::RelayApi(detail) => ErrorHandling::convert(RelayApiError::RelayApi {
                detail: detail.clone(),
            })
            .report_error("relay-api-error"),
            _ => ErrorHandling::convert(RelayApiError::Other {
                reason: self.to_string(),
            })
            .report_error("relay-unexpected-error"),
        }
    }
}

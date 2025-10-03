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

    #[error("Relay API error [{code}]: {detail}")]
    Api { code: String, detail: String },

    #[error("Relay unexpected error: {reason}")]
    Other { reason: String },
}

// Helper for extracting "code" and "detail" from JSON responses
#[derive(Debug, serde::Deserialize)]
struct ApiErrorJson {
    error_code: Option<String>,
    detail: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Relay API error: {0}")]
    RelayApi(String),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HTTP request error: {0}")]
    Viaduct(#[from] viaduct::ViaductError),
    #[error("URL parsing error: {0}")]
    UrlParse(#[from] url::ParseError),
}

impl GetErrorHandling for Error {
    type ExternalError = RelayApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        match self {
            Error::Viaduct(viaduct::ViaductError::NetworkError(e)) => {
                ErrorHandling::convert(RelayApiError::Network {
                    reason: e.to_string(),
                })
                .log_warning()
            }
            Error::RelayApi(detail) => {
                // Try to parse error_code and detail from the provided string
                // Accept both {"error_code", "detail"} or {"detail"} JSON.
                let parsed: Option<ApiErrorJson> = serde_json::from_str(detail).ok();
                let code = parsed
                    .as_ref()
                    .and_then(|j| j.error_code.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                let detail = parsed
                    .as_ref()
                    .and_then(|j| j.detail.clone())
                    .unwrap_or_else(|| detail.clone());
                ErrorHandling::convert(RelayApiError::Api { code, detail })
                    .report_error("relay-api-error")
            }
            _ => ErrorHandling::convert(RelayApiError::Other {
                reason: self.to_string(),
            })
            .report_error("relay-unexpected-error"),
        }
    }
}

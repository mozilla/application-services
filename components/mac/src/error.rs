/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use error_support::{error, ErrorHandling, GetErrorHandling};
use viaduct::Response;

pub type ApiResult<T> = std::result::Result<T, ApiError>;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ApiError {
    #[error("Something unexpected occurred.")]
    Other { reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    #[error("Error requesting ads: {0}")]
    RequestAds(#[from] RequestAdsError),

    #[error("Error recording a click for a placement: {0}")]
    RecordClick(#[from] RecordClickError),

    #[error("Error recording an impressions for a placement: {0}")]
    RecordImpression(#[from] RecordImpressionError),

    #[error("Error reporting an ad: {0}")]
    ReportAd(#[from] ReportAdError),
}

impl GetErrorHandling for ComponentError {
    type ExternalError = ApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        ErrorHandling::convert(ApiError::Other {
            reason: self.to_string(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RequestAdsError {
    #[error("Error building ad requests from configs: {0}")]
    BuildRequest(#[from] BuildRequestError),

    #[error("Error requesting ads from MARS: {0}")]
    FetchAds(#[from] FetchAdsError),

    #[error("Error building placements from ad response: {0}")]
    BuildPlacements(#[from] BuildPlacementsError),
}

#[derive(Debug, thiserror::Error)]
pub enum BuildRequestError {
    #[error("Could not build request with empty placement configs")]
    EmptyConfig,

    #[error("Duplicate placement_id found: {placement_id}. Placement_ids must be unique.")]
    DuplicatePlacementId { placement_id: String },
}

#[derive(Debug, thiserror::Error)]
pub enum BuildPlacementsError {
    #[error("Duplicate placement_id found: {placement_id}. Placement_ids must be unique.")]
    DuplicatePlacementId { placement_id: String },
}

#[derive(Debug, thiserror::Error)]
pub enum FetchAdsError {
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("Error sending request: {0}")]
    Request(#[from] viaduct::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Could not fetch ads, MARS responded with: {0}")]
    HTTPError(#[from] HTTPError),
}

#[derive(Debug, thiserror::Error)]
pub enum EmitTelemetryError {
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("Error sending request: {0}")]
    Request(#[from] viaduct::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Could not fetch ads, MARS responded with: {0}")]
    HTTPError(#[from] HTTPError),
}

#[derive(Debug, thiserror::Error)]
pub enum CallbackRequestError {
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("Error sending request: {0}")]
    Request(#[from] viaduct::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Could not fetch ads, MARS responded with: {0}")]
    HTTPError(#[from] HTTPError),

    #[error("Callback URL missing: {message}")]
    MissingCallback { message: String },
}

#[derive(Debug, thiserror::Error)]
pub enum RecordImpressionError {
    #[error("Callback request to MARS failed: {0}")]
    CallbackRequest(#[from] CallbackRequestError),
}

#[derive(Debug, thiserror::Error)]
pub enum RecordClickError {
    #[error("Callback request to MARS failed: {0}")]
    CallbackRequest(#[from] CallbackRequestError),
}

#[derive(Debug, thiserror::Error)]
pub enum ReportAdError {
    #[error("Callback request to MARS failed: {0}")]
    CallbackRequest(#[from] CallbackRequestError),
}

#[derive(Debug, thiserror::Error)]
pub enum HTTPError {
    #[error("Validation error ({code}): {message}")]
    Validation { code: u16, message: String },

    #[error("Bad request ({code}): {message}")]
    BadRequest { code: u16, message: String },

    #[error("Server error ({code}): {message}")]
    Server { code: u16, message: String },

    #[error("Unexpected error ({code}): {message}")]
    Unexpected { code: u16, message: String },
}

pub fn check_http_status_for_error(response: &Response) -> Result<(), HTTPError> {
    let status = response.status;
    if status >= 400 {
        let error_message = response.text();
        let error = match status {
            400 => HTTPError::BadRequest {
                code: status,
                message: error_message.to_string(),
            },
            422 => HTTPError::Validation {
                code: status,
                message: error_message.to_string(),
            },
            500..=599 => HTTPError::Server {
                code: status,
                message: error_message.to_string(),
            },
            _ => HTTPError::Unexpected {
                code: status,
                message: error_message.to_string(),
            },
        };
        return Err(error);
    }
    Ok(())
}

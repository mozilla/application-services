/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::client::error::ComponentError;
use error_support::{ErrorHandling, GetErrorHandling};

pub type AdsClientApiResult<T> = std::result::Result<T, MozAdsClientApiError>;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MozAdsClientApiError {
    #[error("Something unexpected occurred.")]
    Other { reason: String },
}

impl From<context_id::ApiError> for MozAdsClientApiError {
    fn from(err: context_id::ApiError) -> Self {
        MozAdsClientApiError::Other {
            reason: err.to_string(),
        }
    }
}

impl GetErrorHandling for ComponentError {
    type ExternalError = MozAdsClientApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        ErrorHandling::convert(MozAdsClientApiError::Other {
            reason: self.to_string(),
        })
    }
}

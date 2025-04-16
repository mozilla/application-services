/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error_support::{ErrorHandling, GetErrorHandling};

pub type Result<T> = std::result::Result<T, Error>;

pub type ApiResult<T> = std::result::Result<T, ApiError>;

/// Public error class
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ApiError {
    #[error("Other error: {reason}")]
    Other { reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to construct")]
    FailedToConstruct(String),
}

impl GetErrorHandling for Error {
    /// Public Error type
    type ExternalError = ApiError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        ErrorHandling::convert(ApiError::Other {
            reason: self.to_string(),
        })
    }
}

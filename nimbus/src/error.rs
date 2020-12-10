/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 * */

//! Not complete yet
//! This is where the error definitions can go
//! TODO: Implement proper error handling, this would include defining the error enum,
//! impl std::error::Error using `thiserror` and ensuring all errors are handled appropriately
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid persisted data")]
    InvalidPersistedData,
    #[error("Rkv error: {0}")]
    RkvError(rkv::StoreError),
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("JSON Error: {0}")]
    JSONError(#[from] serde_json::Error),
    #[error("EvaluationError: {0}")]
    EvaluationError(String),
    #[error("Invalid Expression - didn't evaluate to a bool")]
    InvalidExpression,
    #[error("InvalidFractionError: Should be between 0 and 1")]
    InvalidFraction,
    #[error("TryInto error: {0}")]
    TryFromSliceError(#[from] std::array::TryFromSliceError),
    #[error("Empty ratios!")]
    EmptyRatiosError,
    #[error("Attempt to access an element that is out of bounds")]
    OutOfBoundsError,
    #[error("Error parsing URL: {0}")]
    UrlParsingError(#[from] url::ParseError),
    #[error("Error sending request: {0}")]
    RequestError(#[from] viaduct::Error),
    #[error("UUID parsing error: {0}")]
    UuidError(#[from] uuid::Error),
    #[error("Error in network response: {0}")]
    ResponseError(String),
    #[error("Invalid experiments response received")]
    InvalidExperimentResponse,
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("Internal error: {0}")]
    InternalError(&'static str),
    #[error("The experiment {0} does not exist")]
    NoSuchExperiment(String),
    #[error("The branch {0} does not exist for the experiment {1}")]
    NoSuchBranch(String, String),
}

// This can be replaced with #[from] in the enum definition
// once rkv::StoreError impl std::error:Error (https://github.com/mozilla/rkv/issues/188)
impl From<rkv::StoreError> for Error {
    fn from(store_error: rkv::StoreError) -> Self {
        Error::RkvError(store_error)
    }
}

impl<'a> From<jexl_eval::error::EvaluationError<'a>> for Error {
    fn from(eval_error: jexl_eval::error::EvaluationError<'a>) -> Self {
        Error::EvaluationError(eval_error.to_string())
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

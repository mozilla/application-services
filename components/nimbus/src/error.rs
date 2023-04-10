/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 * */

//! Not complete yet
//! This is where the error definitions can go
//! TODO: Implement proper error handling, this would include defining the error enum,
//! impl std::error::Error using `thiserror` and ensuring all errors are handled appropriately

use std::num::{ParseIntError, TryFromIntError};

#[derive(Debug, thiserror::Error)]
pub enum NimbusError {
    #[error("Invalid persisted data")]
    InvalidPersistedData,
    #[cfg(feature = "stateful")]
    #[error("Rkv error: {0}")]
    RkvError(#[from] rkv::StoreError),
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
    #[error("UUID parsing error: {0}")]
    UuidError(#[from] uuid::Error),
    #[error("Invalid experiments response received")]
    InvalidExperimentFormat,
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("Internal error: {0}")]
    InternalError(&'static str),
    #[error("The experiment {0} does not exist")]
    NoSuchExperiment(String),
    #[error("The branch {0} does not exist for the experiment {1}")]
    NoSuchBranch(String, String),
    #[error("Initialization of the database is not yet complete")]
    DatabaseNotReady,
    #[error("Error parsing a sting into a version {0}")]
    VersionParsingError(String),
    #[cfg(feature = "stateful")]
    #[error("Behavior error: {0}")]
    BehaviorError(#[from] BehaviorError),
    #[error("TryFromIntError: {0}")]
    TryFromIntError(#[from] TryFromIntError),
    #[error("ParseIntError: {0}")]
    ParseIntError(#[from] ParseIntError),
    #[error("Transform parameter error: {0}")]
    TransformParameterError(String),
    #[error("Error with HTTP client: {0}")]
    ClientError(#[from] rs_client::ClientError),
}

#[cfg(feature = "stateful")]
#[derive(Debug, thiserror::Error)]
pub enum BehaviorError {
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("Invalid argument: {0}")]
    InvalidDuration(String),
    #[error("IntervalParseError: {0} is not a valid Interval")]
    IntervalParseError(String),
    #[error("The event store is not available on the targeting attributes")]
    MissingEventStore,
}

impl<'a> From<jexl_eval::error::EvaluationError<'a>> for NimbusError {
    fn from(eval_error: jexl_eval::error::EvaluationError<'a>) -> Self {
        NimbusError::EvaluationError(eval_error.to_string())
    }
}

pub type Result<T, E = NimbusError> = std::result::Result<T, E>;

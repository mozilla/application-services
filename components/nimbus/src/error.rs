/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 * */

//! Not complete yet
//! This is where the error definitions can go
//! TODO: Implement proper error handling, this would include defining the error enum,
//! impl std::error::Error using `thiserror` and ensuring all errors are handled appropriately

use std::num::{ParseIntError, TryFromIntError};

// reexport logging helpers.
pub use error_support::{debug, error, info, trace, warn};
#[cfg(feature = "stateful")]
use firefox_versioning::error::VersionParsingError;

#[derive(Debug, thiserror::Error)]
pub enum NimbusError {
    #[error("Initialization of the database is not yet complete")]
    DatabaseNotReady,

    #[error("Empty ratios!")]
    EmptyRatiosError,

    #[error("EvaluationError: {0}")]
    EvaluationError(String),

    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),

    #[error("Internal error: {0}")]
    InternalError(&'static str),

    #[error("Invalid experiment data received")]
    InvalidExperimentFormat,

    #[error("Invalid Expression - didn't evaluate to a bool")]
    InvalidExpression,

    #[error("InvalidFractionError: Should be between 0 and 1")]
    InvalidFraction,

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Invalid persisted data")]
    InvalidPersistedData,

    #[error("JSON Error: {0} — {1}")]
    JSONError(String, String),

    #[error("The branch {0} does not exist for the experiment {1}")]
    NoSuchBranch(String, String),

    #[error("The experiment {0} does not exist")]
    NoSuchExperiment(String),

    #[error("Attempt to access an element that is out of bounds")]
    OutOfBoundsError,

    #[error("ParseIntError: {0}")]
    ParseIntError(#[from] ParseIntError),

    #[error("Transform parameter error: {0}")]
    TransformParameterError(String),

    #[error("TryFromIntError: {0}")]
    TryFromIntError(#[from] TryFromIntError),

    #[error("TryInto error: {0}")]
    TryFromSliceError(#[from] std::array::TryFromSliceError),

    #[error("Error parsing URL: {0}")]
    UrlParsingError(#[from] url::ParseError),

    #[error("UniFFI callback error: {0}")]
    UniFFICallbackError(#[from] uniffi::UnexpectedUniFFICallbackError),

    #[error("UUID parsing error: {0}")]
    UuidError(#[from] uuid::Error),

    #[error("Error parsing a string into a version {0}")]
    VersionParsingError(String),

    // Stateful errors.
    #[cfg(feature = "stateful")]
    #[error("Behavior error: {0}")]
    BehaviorError(#[from] BehaviorError),

    #[cfg(feature = "stateful")]
    #[error("Error with Remote Settings client: {0}")]
    ClientError(#[from] remote_settings::RemoteSettingsError),

    #[cfg(feature = "stateful")]
    #[error("Rkv error: {0}")]
    RkvError(#[from] rkv::StoreError),

    #[cfg(feature = "stateful")]
    #[error("Regex error: {0}")]
    RegexError(#[from] regex::Error),

    // Cirrus-only errors.
    #[cfg(not(feature = "stateful"))]
    #[error("Error in Cirrus: {0}")]
    CirrusError(#[from] CirrusClientError),
}

#[cfg(feature = "stateful")]
#[derive(Debug, thiserror::Error)]
pub enum BehaviorError {
    #[error(r#"EventQueryParseError: "{0}" is not a valid EventQuery"#)]
    EventQueryParseError(String),

    #[error("EventQueryTypeParseError: {0} is not a valid EventQueryType")]
    EventQueryTypeParseError(String),

    #[error("IntervalParseError: {0} is not a valid Interval")]
    IntervalParseError(String),

    #[error("Invalid duration: {0}")]
    InvalidDuration(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("The event store is not available on the targeting attributes")]
    MissingEventStore,

    #[error("The recorded context is not available on the nimbus client")]
    MissingRecordedContext,

    #[error(r#"TypeError: "{0}" is not of type {1}"#)]
    TypeError(String, String),
}

#[cfg(not(feature = "stateful"))]
#[derive(Debug, thiserror::Error)]
pub enum CirrusClientError {
    #[error("Request missing parameter: {0}")]
    RequestMissingParameter(String),
}

#[cfg(test)]
impl From<serde_json::Error> for NimbusError {
    fn from(error: serde_json::Error) -> Self {
        NimbusError::JSONError("test".into(), error.to_string())
    }
}

impl<'a> From<jexl_eval::error::EvaluationError<'a>> for NimbusError {
    fn from(eval_error: jexl_eval::error::EvaluationError<'a>) -> Self {
        NimbusError::EvaluationError(eval_error.to_string())
    }
}

#[cfg(feature = "stateful")]
impl From<VersionParsingError> for NimbusError {
    fn from(eval_error: VersionParsingError) -> Self {
        NimbusError::VersionParsingError(eval_error.to_string())
    }
}

pub type Result<T, E = NimbusError> = std::result::Result<T, E>;

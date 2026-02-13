/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 * */

//! Not complete yet
//! This is where the error definitions can go
//! TODO: Implement proper error handling, this would include defining the error enum,
//! impl std::error::Error using `thiserror` and ensuring all errors are handled appropriately

use std::borrow::Cow;
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

    #[error("UniFFI callback error: {0}")]
    UniFFICallbackError(#[from] uniffi::UnexpectedUniFFICallbackError),

    #[error("Error parsing URL: {0}")]
    UrlParsingError(#[from] url::ParseError),

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
    #[error("Regex error: {0}")]
    RegexError(#[from] regex::Error),

    #[cfg(feature = "stateful")]
    #[error("Rkv error: {0}")]
    RkvError(#[from] rkv::StoreError),

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

/// An Error extension trait that allows simplified error codes to be submitted
/// in telemetry.
pub trait ErrorCode: std::error::Error {
    /// Return the error code for the given error.
    fn error_code(&self) -> Cow<'static, str>;
}

#[cfg(feature = "stateful")]
impl ErrorCode for NimbusError {
    fn error_code(&self) -> Cow<'static, str> {
        match self {
            Self::BehaviorError(e) => format!("BehaviorError({})", e.error_code()).into(),
            Self::ClientError(..) => "ClientError".into(),
            Self::DatabaseNotReady => "DatabaseNotReady".into(),
            Self::EmptyRatiosError => "EmptyRatiosError".into(),
            Self::EvaluationError(..) => "EvaluationError".into(),
            Self::IOError(e) => format!("IOError({:?})", e.kind()).into(),
            Self::InternalError(..) => "InternalError".into(),
            Self::InvalidExperimentFormat => "InvalidExperimentFormat".into(),
            Self::InvalidExpression => "InvalidExpression".into(),
            Self::InvalidFraction => "InvalidFraction".into(),
            Self::InvalidPath(..) => "InvalidPath".into(),
            Self::InvalidPersistedData => "InvalidPersistedData".into(),
            Self::JSONError(..) => "JSONError".into(),
            Self::NoSuchBranch(..) => "NoSuchBranch".into(),
            Self::NoSuchExperiment(..) => "NoSuchExperiment".into(),
            Self::OutOfBoundsError => "OutOfBoundsError".into(),
            Self::ParseIntError(..) => "ParseIntError".into(),
            Self::RegexError(..) => "RegexError".into(),
            Self::RkvError(e) => format!("RkvError({})", e.error_code()).into(),
            Self::TransformParameterError(..) => "TransformParameterError".into(),
            Self::TryFromIntError(..) => "TryFromIntError".into(),
            Self::TryFromSliceError(..) => "TryFromSliceError".into(),
            Self::UniFFICallbackError(..) => "UniFFICallbackError".into(),
            Self::UrlParsingError(..) => "UrlParsingError".into(),
            Self::UuidError(..) => "UuidError".into(),
            Self::VersionParsingError(..) => "VersionParsingError".into(),
        }
    }
}

#[cfg(feature = "stateful")]
impl ErrorCode for rkv::StoreError {
    fn error_code(&self) -> Cow<'static, str> {
        match self {
            Self::ManagerPoisonError => "ManagerPoisonError".into(),
            Self::DatabaseCorrupted => "DatabaseCorrupted".into(),
            Self::KeyValuePairNotFound => "KeyValuePairNotFound".into(),
            Self::KeyValuePairBadSize => "KeyValuePairBadSize".into(),
            Self::FileInvalid => "FileInvalid".into(),
            Self::MapFull => "MapFull".into(),
            Self::DbsFull => "DbsFull".into(),
            Self::ReadersFull => "ReadersFull".into(),
            Self::IoError(e) => format!("IoError({:?})", e.kind()).into(),
            Self::UnsuitableEnvironmentPath(..) => "UnsuitableEnvironmentPath".into(),
            Self::DataError(..) => "DataError".into(),
            Self::SafeModeError(..) => "SafeModeError".into(),
            Self::ReadTransactionAlreadyExists(..) => "ReadTransactionAlreadyExists".into(),
            Self::OpenAttemptedDuringTransaction(..) => "OpenAttemptedDuringTransaction".into(),
        }
    }
}

#[cfg(feature = "stateful")]
impl ErrorCode for BehaviorError {
    fn error_code(&self) -> Cow<'static, str> {
        match self {
            Self::EventQueryParseError(..) => "EventQueryParseError",
            Self::EventQueryTypeParseError(..) => "EventQueryTypeParseError",
            Self::IntervalParseError(..) => "IntervalParseError",
            Self::InvalidDuration(..) => "InvalidDuration",
            Self::InvalidState(..) => "InvalidState",
            Self::MissingEventStore => "MissingEventStore",
            Self::MissingRecordedContext => "MissingRecordedContext",
            Self::TypeError(..) => "TypeError",
        }
        .into()
    }
}

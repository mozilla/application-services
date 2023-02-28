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
pub enum NimbusCoreError {
    #[error("JSON Error: {0}")]
    JSONError(#[from] serde_json::Error),
    #[error("EvaluationError: {0}")]
    EvaluationError(String),
    #[error("Behavior error: {0}")]
    BehaviorError(#[from] BehaviorError),
    #[error("TryFromIntError: {0}")]
    TryFromIntError(#[from] TryFromIntError),
    #[error("ParseIntError: {0}")]
    ParseIntError(#[from] ParseIntError),
    #[error("Transform parameter error: {0}")]
    TransformParameterError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum BehaviorError {
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("IntervalParseError: {0} is not a valid Interval")]
    IntervalParseError(String),
    #[error("The event store is not available on the targeting attributes")]
    MissingEventStore,
}

impl<'a> From<jexl_eval::error::EvaluationError<'a>> for NimbusCoreError {
    fn from(eval_error: jexl_eval::error::EvaluationError<'a>) -> Self {
        NimbusCoreError::EvaluationError(eval_error.to_string())
    }
}

pub type Result<T, E = NimbusCoreError> = std::result::Result<T, E>;

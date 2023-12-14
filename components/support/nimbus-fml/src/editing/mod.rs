/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod error_converter;
mod error_kind;
mod error_path;
mod values_finder;

pub(crate) use error_converter::ErrorConverter;
pub(crate) use error_kind::ErrorKind;
pub(crate) use error_path::ErrorPath;

use crate::error::FMLError;

pub(crate) struct FeatureValidationError {
    pub(crate) path: ErrorPath,
    pub(crate) kind: ErrorKind,
    pub(crate) message: String,
}

impl From<FeatureValidationError> for FMLError {
    fn from(value: FeatureValidationError) -> Self {
        Self::ValidationError(value.path.path, value.message)
    }
}

#[derive(Debug, PartialEq)]
pub struct FmlEditorError {
    pub message: String,
    pub line: u32,
    pub col: u32,
    pub highlight: Option<String>,
}

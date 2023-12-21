/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod cursor_position;
mod error_converter;
mod error_kind;
mod error_path;
mod values_finder;

#[cfg(feature = "client-lib")]
pub(crate) use cursor_position::CursorPosition;
pub(crate) use error_converter::ErrorConverter;
pub(crate) use error_kind::ErrorKind;
pub(crate) use error_path::ErrorPath;

pub(crate) struct FeatureValidationError {
    pub(crate) path: ErrorPath,
    pub(crate) kind: ErrorKind,
}

#[derive(Debug, PartialEq)]
pub struct FmlEditorError {
    pub message: String,
    pub line: u32,
    pub col: u32,
    pub highlight: Option<String>,
}

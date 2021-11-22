/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 * */

//! Not complete yet
//! This is where the error definitions can go
//! TODO: Implement proper error handling, this would include defining the error enum,
//! impl std::error::Error using `thiserror` and ensuring all errors are handled appropriately
#[derive(Debug, thiserror::Error)]
pub enum FMLError {
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("JSON Error: {0}")]
    JSONError(#[from] serde_json::Error),
    #[error("YAML Error: {0}")]
    YAMLError(#[from] serde_yaml::Error),

    #[allow(dead_code)]
    #[error("Can't find file: {0}")]
    InvalidPath(String),

    #[error("Unexpected template problem: {0}")]
    TemplateProblem(#[from] askama::Error),

    #[error("Fatal error: {0}")]
    Fatal(#[from] anyhow::Error),

    #[allow(dead_code)]
    #[error("Internal error: {0}")]
    InternalError(&'static str),
    #[error("Validation Error: {0}")]
    ValidationError(String),
    #[error("Type Parsing Error: {0}")]
    TypeParsingError(String),
}

pub type Result<T, E = FMLError> = std::result::Result<T, E>;

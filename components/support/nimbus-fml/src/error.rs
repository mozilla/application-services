/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 * */

use crate::intermediate_representation::ModuleId;
use std::collections::HashSet;

#[derive(Debug, thiserror::Error)]
pub enum FMLError {
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("JSON Error: {0}")]
    JSONError(#[from] serde_json::Error),
    #[error("YAML Error: {0}")]
    YAMLError(#[from] serde_yaml::Error),
    #[error("URL Error: {0}")]
    UrlError(#[from] url::ParseError),
    #[error("Email Error: {0}")]
    EmailError(#[from] email_address::Error),

    #[error("Fetch Error: {0}")]
    FetchError(#[from] reqwest::Error),
    #[error("Can't find file: {0}")]
    InvalidPath(String),

    #[error("Unexpected template problem: {0}")]
    TemplateProblem(#[from] askama::Error),

    #[error("Fatal error: {0}")]
    Fatal(#[from] anyhow::Error),

    #[error("Internal error: {0}")]
    InternalError(&'static str),
    #[error("Validation Error at {0}: {1}")]
    ValidationError(String, String),
    #[error("Validation Error at {path}: {message}")]
    FeatureValidationError {
        literals: Vec<String>,
        path: String,
        message: String,
    },
    #[error("Type Parsing Error: {0}")]
    TypeParsingError(String),
    #[error("Invalid Channel error: The channel `{0}` is specified, but only {1:?} are supported for the file")]
    InvalidChannelError(String, Vec<String>),

    #[error("Problem with {0}: {1}")]
    FMLModuleError(ModuleId, String),

    #[error("{0}")]
    CliError(String),

    #[cfg(feature = "client-lib")]
    #[error("{0}")]
    ClientError(#[from] ClientError),

    #[error("Feature `{0}` not found on manifest")]
    InvalidFeatureError(String),
}

#[cfg(feature = "client-lib")]
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("{0}")]
    InvalidFeatureConfig(String),
    #[error("{0}")]
    InvalidFeatureId(String),
    #[error("{0}")]
    InvalidFeatureValue(String),
    #[error("{0}")]
    JsonMergeError(String),
}

pub type Result<T, E = FMLError> = std::result::Result<T, E>;

pub(crate) fn did_you_mean(words: HashSet<String>) -> String {
    if words.is_empty() {
        "".to_string()
    } else if words.len() == 1 {
        format!("; did you mean \"{}\"?", words.iter().next().unwrap())
    } else {
        let mut words = words.into_iter().collect::<Vec<_>>();
        let last = words.remove(words.len() - 1);
        format!(
            "; did you mean one of \"{}\" or \"{last}\"?",
            words.join("\", \"")
        )
    }
}

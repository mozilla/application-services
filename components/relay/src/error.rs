/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum Error {
    #[error("Network error: {0}")]
    Network(String),

    #[error("UTF-8 error: {0}")]
    Utf8(String),

    #[error("URL parse error: {0}")]
    UrlParse(String),

    #[error("JSON error: {0}")]
    Json(String),

    #[error("Status error: {0}")]
    Status(String),

    #[error("Relay API returned an error: {0}")]
    RelayApi(String),
}

impl From<url::ParseError> for Error {
    fn from(err: url::ParseError) -> Self {
        Error::RelayApi(format!("URL parse error: {}", err))
    }
}

impl From<viaduct::Error> for Error {
    fn from(err: viaduct::Error) -> Self {
        Error::RelayApi(format!("Viaduct error: {}", err))
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::RelayApi(format!("JSON error: {}", err))
    }
}

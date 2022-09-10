/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use interrupt_support::Interrupted;

#[derive(Debug, thiserror::Error)]
pub enum SyncManagerError {
    #[error("Unknown engine: {0}")]
    UnknownEngine(String),
    #[error("Manager was compiled without support for {0:?}")]
    UnsupportedFeature(String),
    // Used for things like 'failed to decode the provided sync key because it's
    // completely the wrong format', etc.
    #[error("Sync error: {0}")]
    Sync15Error(#[from] sync15::Error),
    #[error("URL parse error: {0}")]
    UrlParseError(#[from] url::ParseError),
    #[error("Operation interrupted")]
    InterruptedError(#[from] Interrupted),
    #[error("Error parsing JSON data: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Logins error: {0}")]
    LoginsError(#[from] logins::LoginsError),
    #[error("Places error: {0}")]
    PlacesError(#[from] places::Error),
    // We should probably upgrade this crate to anyhow, which would mean this
    // gets replaced with AutofillError or similar.
    #[error("External error: {0}")]
    AnyhowError(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, SyncManagerError>;

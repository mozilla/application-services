/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RestmailClientError {
    #[error("Not a restmail account (doesn't end with @restmail.net)")]
    NotARestmailEmail,
    #[error("Max tries reached and the email couldn't be found.")]
    HitRetryMax,
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Error parsing URL: {0}")]
    Disconnect(#[from] url::ParseError),
    #[error("Network error: {0}")]
    RequestError(#[from] viaduct::Error),
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[derive(Debug, thiserror::Error)]
pub(crate) enum InternalError {
    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    #[error(transparent)]
    RequestError(#[from] viaduct::Error),

    #[error(transparent)]
    UnexpectedStatus(#[from] viaduct::UnexpectedStatus),

    #[error(transparent)]
    JsonError(#[from] serde_json::Error),
}

impl InternalError {
    pub fn status(&self) -> Option<u16> {
        match self {
            InternalError::UnexpectedStatus(e) => Some(e.status),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MerinoClientError {
    #[error("Malformed URL: {reason}")]
    BadUrl { reason: String },

    #[error("Failed to fetch suggestions: {reason}")]
    FetchFailed { reason: String, status: Option<u16> },
}

impl From<url::ParseError> for MerinoClientError {
    fn from(err: url::ParseError) -> Self {
        Self::BadUrl {
            reason: err.to_string(),
        }
    }
}

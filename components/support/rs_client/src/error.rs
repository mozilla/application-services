/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// Errors that can occur when using a [crate::Client].
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// An error has occured while sending a request.
    #[error("Error sending request: {0}")]
    RequestError(#[from] viaduct::Error),
    /// An error has occured while parsing an URL.
    #[error("Error parsing URL: {0}")]
    UrlParsingError(#[from] url::ParseError),
    /// The server has asked the client to backoff.
    #[error("Server asked the client to back off ({0} seconds remaining)")]
    BackoffError(u64),
    /// The server returned an error code or the response was unexpected.
    #[error("Error in network response: {0}")]
    ResponseError(String),
}

pub type Result<T, E = ClientError> = std::result::Result<T, E>;

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use interrupt_support::Interrupted;
#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error("Unknown engine: {0}")]
    UnknownEngine(String),
    #[error("Manager was compiled without support for {0:?}")]
    UnsupportedFeature(String),
    #[error("Database connection for '{0}' is not open")]
    ConnectionClosed(String),
    #[error("Handle is invalid: {0}")]
    InvalidHandle(#[from] ffi_support::HandleError),
    #[error("Protobuf decode error: {0}")]
    ProtobufDecodeError(#[from] prost::DecodeError),
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
    LoginsError(#[from] logins::Error),
    #[error("Places error: {0}")]
    PlacesError(#[from] places::Error),
}

error_support::define_error! {
    ErrorKind {
        (InvalidHandle, ffi_support::HandleError),
        (ProtobufDecodeError, prost::DecodeError),
        (Sync15Error, sync15::Error),
        (UrlParseError, url::ParseError),
        (InterruptedError, Interrupted),
        (JsonError, serde_json::Error),
        (LoginsError, logins::Error),
        (PlacesError, places::Error),
    }
}

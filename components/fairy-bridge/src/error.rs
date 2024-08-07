/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub type Result<T> = std::result::Result<T, FairyBridgeError>;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FairyBridgeError {
    #[error("BackendAlreadyInitialized")]
    BackendAlreadyInitialized,
    #[error("NoBackendInitialized")]
    NoBackendInitialized,
    #[error("BackendError({msg})")]
    BackendError { msg: String },
    #[error("HttpError({code})")]
    HttpError { code: u16 },
    #[error("InvalidRequestHeader({name})")]
    InvalidRequestHeader { name: String },
    #[error("InvalidResponseHeader({name})")]
    InvalidResponseHeader { name: String },
    #[error("SerializationError({msg})")]
    SerializationError { msg: String },
}

impl From<serde_json::Error> for FairyBridgeError {
    fn from(e: serde_json::Error) -> Self {
        Self::SerializationError { msg: e.to_string() }
    }
}

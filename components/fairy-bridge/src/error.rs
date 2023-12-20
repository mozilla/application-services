/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub type Result<T, E = FairyBridgeError> = std::result::Result<T, E>;

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
    #[error("InvalidUrl({msg})")]
    InvalidUrl { msg: String },
    #[error("InvalidRequestHeader({name})")]
    InvalidRequestHeader { name: String },
    #[error("InvalidResponseHeader({name})")]
    InvalidResponseHeader { name: String },
    #[error("SerializationError({msg})")]
    SerializationError { msg: String },
}

impl FairyBridgeError {
    pub fn new_backend_error(msg: impl Into<String>) -> Self {
        Self::BackendError { msg: msg.into() }
    }
}

impl From<serde_json::Error> for FairyBridgeError {
    fn from(e: serde_json::Error) -> Self {
        Self::SerializationError { msg: e.to_string() }
    }
}

impl From<url::ParseError> for FairyBridgeError {
    fn from(e: url::ParseError) -> Self {
        Self::InvalidUrl { msg: e.to_string() }
    }
}

pub trait MapBackendError {
    type Ok;

    fn map_backend_error(self) -> Result<Self::Ok>;
}

impl<T, E: ToString> MapBackendError for std::result::Result<T, E> {
    type Ok = T;

    fn map_backend_error(self) -> Result<T> {
        self.map_err(|e| FairyBridgeError::BackendError { msg: e.to_string() })
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for FairyBridgeError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        FairyBridgeError::BackendError {
            msg: error.to_string(),
        }
    }
}

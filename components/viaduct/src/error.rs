/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub type Result<T, E = ViaductError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ViaductError {
    #[error("[no-sentry] Illegal characters in request header '{0}'")]
    RequestHeaderError(String),

    #[error("[no-sentry] Backend error: {0}")]
    BackendError(String),

    #[error("[no-sentry] Network error: {0}")]
    NetworkError(String),

    #[error("The rust-components network backend only be initialized once!")]
    BackendAlreadyInitialized,

    #[error("The rust-components network backend must be initialized before use!")]
    BackendNotInitialized,

    #[error("Backend already initialized.")]
    SetBackendError,

    /// Note: we return this if the server returns a bad URL with
    /// its response. This *probably* should never happen, but who knows.
    #[error("[no-sentry] URL Parse Error: {0}")]
    UrlError(String),

    #[error("[no-sentry] Validation error: URL does not use TLS protocol.")]
    NonTlsUrl,

    #[error("OHTTP channel '{0}' is not configured")]
    OhttpChannelNotConfigured(String),

    #[error("Failed to fetch OHTTP config: {0}")]
    OhttpConfigFetchFailed(String),

    #[error("OHTTP request error: {0}")]
    OhttpRequestError(String),

    #[error("OHTTP response error: {0}")]
    OhttpResponseError(String),

    #[error("OHTTP support is not enabled in this build")]
    OhttpNotSupported,
}

impl ViaductError {
    pub fn new_backend_error(msg: impl Into<String>) -> Self {
        Self::BackendError(msg.into())
    }
}

impl From<url::ParseError> for ViaductError {
    fn from(e: url::ParseError) -> Self {
        ViaductError::UrlError(e.to_string())
    }
}

/// This error is returned as the `Err` result from
/// [`Response::require_success`].
///
/// Note that it's not a variant on `Error` to distinguish between errors
/// caused by the network, and errors returned from the server.
#[derive(thiserror::Error, Debug, Clone)]
#[error("Error: {method} {url} returned {status}")]
pub struct UnexpectedStatus {
    pub status: u16,
    pub method: crate::Method,
    pub url: url::Url,
}

/// Map errors from external crates like `tokio` and `hyper` to `Error::BackendError`
///
/// This works for any error that implements ToString
pub trait MapBackendError {
    type Ok;

    fn map_backend_error(self) -> Result<Self::Ok>;
}

impl<T, E: ToString> MapBackendError for std::result::Result<T, E> {
    type Ok = T;

    fn map_backend_error(self) -> Result<T> {
        self.map_err(|e| ViaductError::BackendError(e.to_string()))
    }
}

/// Implement From<UnexpectedUniFFICallbackError> so that unexpected errors when invoking backend
/// callback interface methods get converted to `BackendError`.
impl From<uniffi::UnexpectedUniFFICallbackError> for ViaductError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        ViaductError::BackendError(error.to_string())
    }
}

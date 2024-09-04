/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{FairyBridgeError, Request, Response};
use std::sync::Arc;

/// Settings for a backend instance
///
/// Backend constructions should input this in order to configure themselves
///
/// Repr(C) so we can pass it to the C backend
#[derive(Debug, uniffi::Record)]
#[repr(C)]
pub struct BackendSettings {
    // Connection timeout in ms (0 indicates no timeout).
    #[uniffi(default = 0)]
    pub connect_timeout: u32,
    // Timeout for the entire request in ms (0 indicates no timeout).
    #[uniffi(default = 0)]
    pub timeout: u32,
    // Maximum amount of redirects to follow (0 means redirects are not allowed)
    #[uniffi(default = 10)]
    pub redirect_limit: u32,
}

#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait Backend: Send + Sync {
    async fn send_request(self: Arc<Self>, request: Request) -> Result<Response, FairyBridgeError>;
}

#[uniffi::export]
pub fn init_backend(backend: Arc<dyn Backend>) -> Result<(), FairyBridgeError> {
    crate::REGISTERED_BACKEND
        .set(backend)
        .map_err(|_| FairyBridgeError::BackendAlreadyInitialized)
}

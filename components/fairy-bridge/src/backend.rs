/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{FairyBridgeError, Request, Response};
use std::sync::Arc;

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

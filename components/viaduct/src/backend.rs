/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
*
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::sync::{Arc, OnceLock};

use crate::{ClientSettings, Request, Response, Result, ViaductError};

#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait Backend: Send + Sync + 'static {
    async fn send_request(&self, request: Request, settings: ClientSettings) -> Result<Response>;
}

static REGISTERED_BACKEND: OnceLock<Arc<dyn Backend>> = OnceLock::new();

#[uniffi::export]
pub fn init_backend(backend: Arc<dyn Backend>) -> Result<()> {
    REGISTERED_BACKEND
        .set(backend)
        .map_err(|_| ViaductError::BackendAlreadyInitialized)
}

pub fn get_backend() -> Result<&'static Arc<dyn Backend>> {
    REGISTERED_BACKEND
        .get()
        .ok_or(ViaductError::BackendNotInitialized)
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
*
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Right now we're in a transition period where we have 2 backend traits.  The old `Backend`
// trait is defined in `backend.rs` and the new `Backend` trait is bdefined here
//
// The new backend trait has a few of improvements to the old backend trait:
//   - UniFFI-compatible
//   - async-based
//   - Inputs per-request settings for things like timeouts, rather than using global values
//
// See `README.md` for details about the transition from the old to new API.

use std::sync::{Arc, OnceLock};

use crate::{
    backend as old_backend, settings::GLOBAL_SETTINGS, ClientSettings, Request, Response, Result,
    ViaductError,
};

#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait Backend: Send + Sync + 'static {
    async fn send_request(&self, request: Request, settings: ClientSettings) -> Result<Response>;
}

static REGISTERED_BACKEND: OnceLock<Arc<dyn Backend>> = OnceLock::new();

#[uniffi::export]
pub fn init_backend(backend: Arc<dyn Backend>) -> Result<()> {
    old_backend::set_backend(Box::leak(Box::new(backend.clone())))?;
    REGISTERED_BACKEND
        .set(backend)
        .map_err(|_| ViaductError::BackendAlreadyInitialized)
}

pub fn get_backend() -> Result<&'static Arc<dyn Backend>> {
    REGISTERED_BACKEND
        .get()
        .ok_or(ViaductError::BackendNotInitialized)
}

impl old_backend::Backend for Arc<dyn Backend> {
    fn send(&self, request: crate::Request) -> Result<crate::Response, crate::ViaductError> {
        let settings = GLOBAL_SETTINGS.read();
        let client_settings = ClientSettings {
            timeout: match settings.read_timeout {
                Some(d) => d.as_millis() as u32,
                None => 0,
            },
            redirect_limit: if settings.follow_redirects { 10 } else { 0 },
            #[cfg(feature = "ohttp")]
            ohttp_channel: None,
        };
        pollster::block_on(self.send_request(request, client_settings))
    }
}

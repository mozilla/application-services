/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{new_backend::get_backend, settings::validate_request, Request, Response, Result};

/// HTTP Client
///
/// This represents the "new" API.
/// See `README.md` for details about the transition from the old to new API.
#[derive(Default)]
pub struct Client {
    settings: ClientSettings,
}

impl Client {
    pub fn new(settings: ClientSettings) -> Self {
        Self { settings }
    }

    pub async fn send(&self, request: Request) -> Result<Response> {
        validate_request(&request)?;
        get_backend()?
            .send_request(request, self.settings.clone())
            .await
    }

    pub fn send_sync(&self, request: Request) -> Result<Response> {
        pollster::block_on(self.send(request))
    }
}

#[derive(Debug, uniffi::Record, Clone)]
#[repr(C)]
pub struct ClientSettings {
    // Timeout for the entire request in ms (0 indicates no timeout).
    #[uniffi(default = 0)]
    pub timeout: u32,
    // Maximum amount of redirects to follow (0 means redirects are not allowed)
    #[uniffi(default = 10)]
    pub redirect_limit: u32,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            #[cfg(target_os = "ios")]
            timeout: 7000,
            #[cfg(not(target_os = "ios"))]
            timeout: 10000,
            redirect_limit: 10,
        }
    }
}

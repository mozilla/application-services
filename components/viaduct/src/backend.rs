/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{info, settings::validate_request, trace};
use ffi::FfiBackend;
use once_cell::sync::OnceCell;
mod ffi;

pub fn note_backend(which: &str) {
    // If trace logs are enabled: log on every request. Otherwise, just log on
    // the first request at `info` level. We remember if the Once was triggered
    // to avoid logging twice in the first case.
    static NOTE_BACKEND_ONCE: std::sync::Once = std::sync::Once::new();
    let mut called = false;
    NOTE_BACKEND_ONCE.call_once(|| {
        info!("Using HTTP backend {}", which);
        called = true;
    });
    if !called {
        trace!("Using HTTP backend {}", which);
    }
}

pub trait Backend: Send + Sync + 'static {
    fn send(&self, request: crate::Request) -> Result<crate::Response, crate::ViaductError>;
}

static BACKEND: OnceCell<&'static dyn Backend> = OnceCell::new();

pub fn set_backend(b: &'static dyn Backend) -> Result<(), crate::ViaductError> {
    BACKEND
        .set(b)
        .map_err(|_| crate::error::ViaductError::SetBackendError)
}

pub(crate) fn get_backend() -> &'static dyn Backend {
    *BACKEND.get_or_init(|| Box::leak(Box::new(FfiBackend)))
}

pub fn send(request: crate::Request) -> Result<crate::Response, crate::ViaductError> {
    validate_request(&request)?;
    get_backend().send(request)
}

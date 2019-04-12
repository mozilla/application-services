/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "reqwest")]
mod reqwest;

mod ffi;

// We allow globally forcing us to use the FFI backend for better
// testing, for example.
static FFI_FORCED: AtomicBool = AtomicBool::new(false);

fn ffi_is_forced() -> bool {
    FFI_FORCED.load(Ordering::SeqCst)
}

pub fn force_enable_ffi_backend(v: bool) {
    FFI_FORCED.store(v, Ordering::SeqCst)
}

pub(crate) fn note_backend(which: &str) {
    // If trace logs are enabled: log on every request. Otherwise, just log on
    // the first request at `info` level. We remember if the Once was triggered
    // to avoid logging twice in the first case.
    static NOTE_BACKEND_ONCE: std::sync::Once = std::sync::Once::new();
    let mut called = false;
    NOTE_BACKEND_ONCE.call_once(|| {
        log::info!("Using HTTP backend {}", which);
        called = true;
    });
    if !called {
        log::trace!("Using HTTP backend {}", which);
    }
}

pub fn send(request: crate::Request) -> Result<crate::Response, crate::Error> {
    if ffi_is_forced() {
        return self::ffi::send(request);
    }
    #[cfg(feature = "reqwest")]
    {
        self::reqwest::send(request)
    }
    #[cfg(not(feature = "reqwest"))]
    {
        self::ffi::send(request)
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::sync::atomic::{AtomicBool, Ordering, ATOMIC_BOOL_INIT};

#[cfg(feature = "rust-http-stack")]
mod reqwest;

mod ffi;

// We allow globally forcing us to use the FFI backend for better
// testing, for example.
static FFI_FORCED: AtomicBool = ATOMIC_BOOL_INIT;

fn ffi_is_forced() -> bool {
    FFI_FORCED.load(Ordering::SeqCst)
}

pub fn force_enable_ffi_backend(v: bool) {
    FFI_FORCED.store(v, Ordering::SeqCst)
}

pub fn send(request: crate::Request) -> Result<crate::Response, crate::Error> {
    if ffi_is_forced() {
        return self::ffi::send(request);
    }
    #[cfg(feature = "rust-http-stack")]
    {
        self::reqwest::send(request)
    }
    #[cfg(not(feature = "rust-http-stack"))]
    {
        self::ffi::send(request)
    }
}

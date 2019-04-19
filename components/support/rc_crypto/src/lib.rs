/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// This crate provides all the cryptographic primitives required by
/// this workspace, backed by the NSS library.
/// The exposed API is pretty much the same as the `ring` crate
/// (https://briansmith.org/rustdoc/ring/) as it is well thought.
pub mod constant_time;
pub mod digest;
mod error;
#[cfg(feature = "hawk")]
mod hawk_crypto;
pub mod hkdf;
pub mod hmac;
#[cfg(not(target_os = "ios"))]
mod p11;
pub mod rand;
#[cfg(not(target_os = "ios"))]
mod util;

// Expose `hawk` if the hawk feature is on. This avoids consumers needing to
// configure this separately, which is more or less trivial to do incorrectly.
#[cfg(feature = "hawk")]
pub use hawk;

pub use crate::error::{Error, ErrorKind, Result};

/// Only required to be called if you intend to use this library in conjunction
/// with the `hawk` crate.
pub fn init_once() {
    #[cfg(all(target_os = "ios", feature = "hawk"))]
    {
        static INIT_ONCE: std::sync::Once = std::sync::Once::new();
        INIT_ONCE.call_once(hawk_crypto::init);
    }
    #[cfg(not(target_os = "ios"))]
    {
        crate::util::ensure_nss_initialized();
    }
}

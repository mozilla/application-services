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
pub mod hkdf;
pub mod hmac;
#[cfg(not(target_os = "ios"))]
mod p11;
pub mod rand;
#[cfg(not(target_os = "ios"))]
mod util;

pub use crate::error::{Error, ErrorKind, Result};

/* Permission to use, copy, modify, and/or distribute this software for any
 * purpose with or without fee is hereby granted, provided that the above
 * copyright notice and this permission notice appear in all copies.
 *
 * THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHORS DISCLAIM ALL WARRANTIES
 * WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY
 * SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
 * WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
 * OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
 * CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE. */

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

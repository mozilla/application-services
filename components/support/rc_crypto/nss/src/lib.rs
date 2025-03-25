/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]
#[macro_use]
mod util;
pub mod aes;
pub mod cert;
pub mod ec;
pub mod ecdh;
mod error;
pub mod pbkdf2;
pub mod pk11;
pub mod pkixc;
pub mod secport;
pub use crate::error::{Error, ErrorKind, Result};
pub use util::assert_nss_initialized as assert_initialized;
pub use util::ensure_nss_initialized as ensure_initialized;

#[cfg(feature = "keydb")]
pub use util::ensure_nss_initialized_with_profile_dir as ensure_initialized_with_profile_dir;

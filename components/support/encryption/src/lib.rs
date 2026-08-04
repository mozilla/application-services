/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

#[macro_use]
mod error;

mod encryption;

pub use crate::encryption::{
    EncryptorDecryptor, KeyManager, ManagedEncryptorDecryptor, StaticKeyManager,
};

#[cfg(feature = "keydb")]
pub use crate::encryption::{NSSKeyManager, PrimaryPasswordAuthenticator};

pub use crate::encryption::{check_canary, create_canary, create_key};
pub use crate::error::*;
use std::sync::Arc;

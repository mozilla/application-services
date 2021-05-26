/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

pub mod db;
pub mod encryption;
pub mod error;
pub mod sync;

// Re-export stuff the sync manager needs.
pub use crate::db::store::{Store, STORE_FOR_MANAGER};

// Expose stuff needed by the uniffi generated code.
use crate::db::models::address::*;
use crate::db::models::credit_card::*;
use crate::encryption::{create_key, decrypt_string, encrypt_string};
use error::Error;

include!(concat!(env!("OUT_DIR"), "/autofill.uniffi.rs"));

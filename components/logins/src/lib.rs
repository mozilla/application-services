/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

#[macro_use]
mod error;
mod login;

mod db;
pub mod encryption;
mod engine;
mod migrate_sqlcipher_db;
pub mod schema;
mod store;
mod update_plan;
mod util;

mod ffi;

// Mostly exposed for the sync manager.
pub use crate::db::LoginDb;
pub use crate::engine::LoginsSyncEngine;
pub use crate::error::*;
pub use crate::login::*;
pub use crate::store::*;

pub mod msg_types {
    include!("mozilla.appservices.logins.protobuf.rs");
}

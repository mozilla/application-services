/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

#[macro_use]
mod error;
mod login;

mod db;
mod engine;
mod schema;
mod store;
mod update_plan;
mod util;

uniffi_macros::include_scaffolding!("logins");

pub use crate::db::{
    open_and_get_salt, open_and_migrate_to_plaintext_header, LoginDb, MigrationMetrics,
    MigrationPhaseMetrics,
};
pub use crate::engine::LoginsSyncEngine;
pub use crate::error::*;
pub use crate::login::*;
pub use crate::store::*;

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

#[macro_use]
mod error;
mod login;

pub mod db;
pub mod schema;
mod store;
mod update_plan;
mod util;

uniffi_macros::include_scaffolding!("logins");

// Mostly exposed for the sync manager.
use crate::db::open_and_get_salt;
use crate::db::open_and_migrate_to_plaintext_header;
pub use crate::db::LoginDb;
pub use crate::db::LoginStore;
use crate::db::MigrationMetrics;
use crate::db::MigrationPhaseMetrics;
pub use crate::error::*;
pub use crate::login::*;
pub use crate::store::*;

#[derive(Clone, PartialEq)]
pub struct PasswordInfo {
    pub id: std::string::String,
    pub hostname: std::string::String,
    pub password: std::string::String,
    pub username: std::string::String,
    pub http_realm: ::std::option::Option<std::string::String>,
    pub form_submit_url: ::std::option::Option<std::string::String>,
    pub username_field: std::string::String,
    pub password_field: std::string::String,
    pub times_used: i64,
    pub time_created: i64,
    pub time_last_used: i64,
    pub time_password_changed: i64,
}
#[derive(Clone, PartialEq)]
pub struct PasswordInfos {
    pub infos: ::std::vec::Vec<PasswordInfo>,
}

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
pub mod migrate_sqlcipher_db;
mod schema;
mod store;
mod sync;
mod util;

uniffi_macros::include_scaffolding!("logins");

pub use crate::db::{LoginDb, MigrationMetrics, MigrationPhaseMetrics};
use crate::encryption::create_key;
pub use crate::error::*;
pub use crate::login::*;
pub use crate::migrate_sqlcipher_db::migrate_logins;
pub use crate::store::*;
pub use crate::sync::LoginsSyncEngine;

// Public encryption functions.  We create need to publish these as top-level functions to expose
// them across UniFFI
fn encrypt_login(login: Login, enc_key: &str) -> Result<EncryptedLogin> {
    let encdec = encryption::EncryptorDecryptor::new(enc_key)?;
    login.encrypt(&encdec)
}

fn decrypt_login(login: EncryptedLogin, enc_key: &str) -> Result<Login> {
    let encdec = encryption::EncryptorDecryptor::new(enc_key)?;
    login.decrypt(&encdec)
}

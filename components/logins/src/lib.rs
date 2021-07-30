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
mod migrate_sqlcipher_db;
mod schema;
mod store;
mod sync;
mod util;

uniffi_macros::include_scaffolding!("logins");

pub use crate::db::{LoginDb, MigrationMetrics, MigrationPhaseMetrics};
use crate::encryption::create_key;
pub use crate::error::*;
pub use crate::login::*;
pub use crate::store::*;
pub use crate::sync::LoginsSyncEngine;

// public encryption functions - not in encryption.rs as theoretically that file doesn't need to
// know about any of our structs.
fn encrypt_fields(fields: &SecureLoginFields, enc_key: &str) -> Result<String> {
    let encdec = encryption::EncryptorDecryptor::new(enc_key)?;
    encdec.encrypt_struct(fields)
}

fn decrypt_fields(ciphertext: &str, enc_key: &str) -> Result<SecureLoginFields> {
    let encdec = encryption::EncryptorDecryptor::new(enc_key)?;
    encdec.decrypt_struct(ciphertext)
}

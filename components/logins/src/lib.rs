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

uniffi::include_scaffolding!("logins");

pub use crate::db::LoginDb;
use crate::encryption::{check_canary, create_canary, create_key};
pub use crate::error::*;
pub use crate::login::*;
pub use crate::migrate_sqlcipher_db::migrate_logins;
pub use crate::store::*;
pub use crate::sync::LoginsSyncEngine;

// Public encryption functions.  We publish these as top-level functions to expose them across
// UniFFI
#[handle_error(Error)]
fn encrypt_login(login: Login, enc_key: &str) -> ApiResult<EncryptedLogin> {
    let encdec = encryption::EncryptorDecryptor::new(enc_key)?;
    login.encrypt(&encdec)
}

#[handle_error(Error)]
fn decrypt_login(login: EncryptedLogin, enc_key: &str) -> ApiResult<Login> {
    let encdec = encryption::EncryptorDecryptor::new(enc_key)?;
    login.decrypt(&encdec)
}

#[handle_error(Error)]
fn encrypt_fields(sec_fields: SecureLoginFields, enc_key: &str) -> ApiResult<String> {
    let encdec = encryption::EncryptorDecryptor::new(enc_key)?;
    sec_fields.encrypt(&encdec)
}

#[handle_error(Error)]
fn decrypt_fields(sec_fields: String, enc_key: &str) -> ApiResult<SecureLoginFields> {
    let encdec = encryption::EncryptorDecryptor::new(enc_key)?;
    SecureLoginFields::decrypt(&sec_fields, &encdec)
}

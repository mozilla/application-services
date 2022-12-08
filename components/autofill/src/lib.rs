/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

pub mod db;
/// This is the *local* encryption support - it has nothing to do with the
/// encryption used by sync.

/// For context, what "local encryption" means in this context is:
/// * We use regular sqlite, but want to ensure the credit-card numbers are
///   encrypted in the DB - so we store the number encrypted, and the key
///   is managed by the app.
/// * The credit-card API always just accepts and returns the encrypted string,
///   so we also expose encryption and decryption public functions that take
///   the key and text. The core storage API never knows the unencrypted number.
///
/// This makes life tricky for Sync - sync has its own encryption and its own
/// management of sync keys. The entire records are encrypted on the server -
/// so the record on the server has the plain-text number (which is then
/// encrypted as part of the entire record), so:
/// * When transforming a record from the DB into a Sync record, we need to
///   *decrypt* the field.
/// * When transforming a record from Sync into a DB record, we need to *encrypt*
///   the field.
///
/// So Sync needs to know the key etc, and that needs to get passed down
/// multiple layers, from the app saying "sync now" all the way down to the
/// low level sync code.
/// To make life a little easier, we do that via a struct.
pub mod encryption;
pub mod error;
pub mod sync;

// Re-export stuff the sync manager needs.
pub use crate::db::store::get_registered_sync_engine;

// Expose stuff needed by the uniffi generated code.
use crate::db::models::address::*;
use crate::db::models::credit_card::*;
use crate::db::store::Store;
use crate::encryption::{create_key, decrypt_string, encrypt_string};
pub use error::{ApiResult, AutofillApiError, Error, Result};

include!(concat!(env!("OUT_DIR"), "/autofill.uniffi.rs"));

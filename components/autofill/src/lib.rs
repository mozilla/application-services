/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

pub mod db;
pub mod error;

// Expose stuff needed by the uniffi generated code.
// use db::models::address::*;
use db::models::credit_card::*;
// use error::ErrorKind;
// use db::store::Store;

include!(concat!(env!("OUT_DIR"), "/autofill.uniffi.rs"));

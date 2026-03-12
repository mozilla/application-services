/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]
#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]

mod bindings;
pub use bindings::*;

// So we link against the SQLite lib imported by parent crates
// such as places and logins. For finer control over how that is
// chosen, you can enable features on that crate (eg, `bundled and `in_gecko`
// in particular are likely to be useful.)
#[allow(unused_extern_crates)]
extern crate libsqlite3_sys;

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate sync15_adapter as sync;

#[macro_use]
extern crate log;

#[cfg(test)]
extern crate env_logger;

#[macro_use]
extern crate lazy_static;

extern crate failure;

// reversing a unicode string is more difficult than it sounds!
extern crate unicode_segmentation;

#[macro_use]
extern crate failure_derive;

#[macro_use]
extern crate more_asserts;

extern crate url;

extern crate rusqlite;

extern crate serde;
extern crate serde_json;

#[macro_use]
extern crate serde_derive;

#[macro_use]
pub mod schema;
pub mod types;
// XXX - somehow we want to reuse db.rs across these libs
pub mod db;
//pub mod util;

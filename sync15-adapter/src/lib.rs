/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// `error_chain!` can recurse deeply and I guess we're just supposed to live with that...
#![recursion_limit = "1024"]

extern crate serde;
extern crate base64;
extern crate openssl;
extern crate reqwest;
extern crate hawk;
#[macro_use]
extern crate hyper;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate log;

// Right now we only need the `json!` macro in tests, and a raw `#[macro_use]` gives us a warning
#[cfg_attr(test, macro_use)]
extern crate serde_json;

#[macro_use]
extern crate error_chain;

extern crate url;
extern crate base16;

// TODO: Some of these don't need to be pub...
pub mod key_bundle;
pub mod error;
pub mod bso_record;
pub mod record_types;
pub mod token;
pub mod collection_keys;
pub mod util;
pub mod request;
pub mod service;
pub mod tombstone;

// Re-export some of the types callers are likely to want for convenience.
pub use bso_record::{BsoRecord, Sync15Record};
pub use tombstone::{MaybeTombstone, Tombstone, NonTombstone};
pub use service::{Sync15ServiceInit, Sync15Service, CollectionUpdate};
pub use error::{Result, Error, ErrorKind};

pub use MaybeTombstone::*;

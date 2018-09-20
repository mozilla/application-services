/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate serde;
extern crate base64;
extern crate openssl;
extern crate reqwest;
extern crate hawk;
extern crate hyper;

extern crate failure;

#[macro_use]
extern crate failure_derive;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate log;

#[cfg_attr(test, macro_use)]
extern crate serde_json;

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
pub mod changeset;
pub mod sync;
pub mod client;
pub mod state;

// Re-export some of the types callers are likely to want for convenience.
pub use bso_record::{BsoRecord, EncryptedBso, Payload, CleartextBso};
pub use changeset::{RecordChangeset, IncomingChangeset, OutgoingChangeset};
pub use error::{Result, Error, ErrorKind};
pub use sync::{synchronize, Store};
pub use util::{ServerTimestamp, SERVER_EPOCH};
pub use key_bundle::KeyBundle;
pub use client::{Sync15StorageClientInit, Sync15StorageClient};
pub use state::{GlobalState, SetupStateMachine};

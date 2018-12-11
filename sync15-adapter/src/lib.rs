/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate base64;
extern crate hawk;
extern crate hyper;
extern crate openssl;
extern crate reqwest;
extern crate serde;

extern crate failure;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate log;

#[cfg_attr(test, macro_use)]
extern crate serde_json;

extern crate base16;
extern crate url;

// TODO: Some of these don't need to be pub...
pub mod bso_record;
pub mod changeset;
pub mod client;
pub mod collection_keys;
pub mod error;
pub mod key_bundle;
pub mod record_types;
pub mod request;
pub mod state;
pub mod sync;
pub mod sync_multiple;
pub mod token;
pub mod util;

// Re-export some of the types callers are likely to want for convenience.
pub use bso_record::{BsoRecord, CleartextBso, EncryptedBso, Payload};
pub use changeset::{IncomingChangeset, OutgoingChangeset, RecordChangeset};
pub use client::{Sync15StorageClient, Sync15StorageClientInit};
pub use error::{Error, ErrorKind, Result};
pub use key_bundle::KeyBundle;
pub use request::CollectionRequest;
pub use state::{GlobalState, SetupStateMachine};
pub use sync::{synchronize, Store};
pub use sync_multiple::{sync_multiple, ClientInfo};
pub use util::{ServerTimestamp, SERVER_EPOCH};

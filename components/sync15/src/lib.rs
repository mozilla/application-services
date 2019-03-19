/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]

mod bso_record;
mod changeset;
mod client;
mod collection_keys;
mod error;
mod key_bundle;
mod record_types;
mod request;
mod state;
mod sync;
mod sync_multiple;
pub mod telemetry;
mod token;
mod util;

// Re-export some of the types callers are likely to want for convenience.
pub use crate::bso_record::{BsoRecord, CleartextBso, EncryptedBso, EncryptedPayload, Payload};
pub use crate::changeset::{IncomingChangeset, OutgoingChangeset, RecordChangeset};
pub use crate::client::{SetupStorageClient, Sync15StorageClient, Sync15StorageClientInit};
pub use crate::error::{Error, ErrorKind, Result};
pub use crate::key_bundle::KeyBundle;
pub use crate::request::CollectionRequest;
pub use crate::state::{GlobalState, SetupStateMachine};
pub use crate::sync::{synchronize, Store};
pub use crate::sync_multiple::{sync_multiple, ClientInfo};
pub use crate::util::{random_guid, ServerTimestamp, SERVER_EPOCH};

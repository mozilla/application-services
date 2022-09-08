/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints, clippy::implicit_hasher)]
#![warn(rust_2018_idioms)]

pub mod changeset;
mod client;
pub mod clients;
mod coll_state;
mod collection_keys;
mod error;
mod record_types;
mod request;
mod state;
mod status;
mod sync;
mod sync_multiple;
pub mod telemetry;
mod token;
mod util;

// Re-export some of the types callers are likely to want for convenience.
pub use crate::changeset::{IncomingChangeset, OutgoingChangeset, RecordChangeset};
pub use crate::client::{
    SetupStorageClient, Sync15ClientResponse, Sync15StorageClient, Sync15StorageClientInit,
};
pub use crate::coll_state::{CollState, CollSyncIds, EngineSyncAssociation};
pub use crate::collection_keys::CollectionKeys;
pub use crate::error::{Error, Result};
pub use crate::request::CollectionRequest;
pub use crate::state::{GlobalState, SetupStateMachine};
pub use crate::status::{ServiceStatus, SyncResult};
pub use crate::sync::{synchronize, SyncEngine};
pub use crate::sync_multiple::{
    sync_multiple, sync_multiple_with_command_processor, MemoryCachedState, SyncRequestInfo,
};
pub use sync15_traits::client::DeviceType;
pub use sync15_traits::SyncTraitsError;

pub use crate::util::ServerTimestamp;
pub use sync15_traits::{
    BsoRecord, CleartextBso, EncryptedBso, EncryptedPayload, KeyBundle, Payload, SyncEngineId,
};

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints, clippy::implicit_hasher)]
#![warn(rust_2018_idioms)]

pub mod client;
// Note that `clients` should probably be in `sync_client`, but let's not make
// things too nested at this stage...
pub mod clients;
mod collection_keys;
mod error;
pub(crate) mod record_types;
pub mod telemetry;

pub use crate::collection_keys::CollectionKeys;
pub use crate::error::{Error, Result};
pub use sync15_traits::client::DeviceType;
pub use sync15_traits::SyncTraitsError;

pub use sync15_traits::{EncryptedPayload, KeyBundle};

pub use sync15_traits::{
    CleartextBso, CollSyncIds, CollectionRequest, EngineSyncAssociation, IncomingChangeset,
    OutgoingChangeset, Payload, RecordChangeset, ServerTimestamp, SyncEngine, SyncEngineId,
};

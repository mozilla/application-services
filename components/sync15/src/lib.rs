/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints, clippy::implicit_hasher)]
#![warn(rust_2018_idioms)]

#[cfg(feature = "sync-client")]
pub mod client;
// Types to describe client records
mod client_types;
// Note that `clients` should probably be in `sync_client`, but let's not make
// things too nested at this stage...
#[cfg(feature = "sync-client")]
pub mod clients;
#[cfg(feature = "sync-client")]
mod collection_keys;
mod error;
pub(crate) mod record_types;

#[cfg(feature = "sync-engine")]
pub mod engine;

pub mod telemetry;

pub use crate::client_types::{ClientData, DeviceType, RemoteClient};
#[cfg(feature = "sync-client")]
pub use crate::collection_keys::CollectionKeys;
pub use crate::error::{Error, Result};
pub use sync15_traits::SyncTraitsError;

pub use sync_guid::Guid;

#[cfg(feature = "crypto")]
pub use sync15_traits::{BsoRecord, CleartextBso, EncryptedBso, EncryptedPayload, KeyBundle};

pub use sync15_traits::{Payload, ServerTimestamp};

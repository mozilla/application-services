/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

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
pub mod telemetry;

// Re-export some of the types callers are likely to want for convenience.
pub use crate::bso_record::{BsoRecord, CleartextBso, EncryptedBso, Payload};
pub use crate::changeset::{IncomingChangeset, OutgoingChangeset, RecordChangeset};
pub use crate::client::{Sync15StorageClient, Sync15StorageClientInit};
pub use crate::error::{Error, ErrorKind, Result};
pub use crate::key_bundle::KeyBundle;
pub use crate::request::CollectionRequest;
pub use crate::state::{GlobalState, SetupStateMachine};
pub use crate::sync::{synchronize, Store};
pub use crate::sync_multiple::{sync_multiple, ClientInfo};
pub use crate::util::{ServerTimestamp, SERVER_EPOCH};

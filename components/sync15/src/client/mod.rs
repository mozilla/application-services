/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! A module for everything needed to be a "sync client" - ie, a device which
//! can perform a full sync of any number of collections, including managing
//! the server state.
mod coll_state;
mod coll_update;
mod request;
mod state;
mod status;
mod storage_client;
mod sync;
mod sync_multiple;
mod token;
mod util;

pub(crate) use coll_state::CollState;
pub(crate) use coll_update::{fetch_incoming, CollectionUpdate};
pub(crate) use request::InfoConfiguration;
pub(crate) use state::GlobalState;
pub use status::{ServiceStatus, SyncResult};
pub use storage_client::{
    SetupStorageClient, Sync15ClientResponse, Sync15StorageClient, Sync15StorageClientInit,
};
pub use sync_multiple::{
    sync_multiple, sync_multiple_with_command_processor, MemoryCachedState, SyncRequestInfo,
};

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! A LegacySyncEngine isn't that far removed from the "modern" `SyncEngine`.
//! This module helps adapt one to the other, and should be killed; it's largely
//! mechanical to migrate a `LegacySyncEngine` to a `SyncEngine`.

use super::sync_engine::{EngineSyncAssociation, SyncEngine};
use super::CollectionRequest;
use crate::bso::{IncomingBso, OutgoingBso};
use crate::client_types::ClientData;
use crate::{telemetry, CollectionName, Guid, ServerTimestamp};
use anyhow::Result;
use std::cell::{Cell, RefCell};

/// A Changeset is a concept that only exists for the LegacySyncEngine.
// Incoming and Outgoing changesets are almost identical except for the timestamp.
#[derive(Debug)]
pub struct IncomingChangeset {
    pub changes: Vec<IncomingBso>,
    /// The server timestamp of the collection.
    pub timestamp: ServerTimestamp,
    pub collection: CollectionName,
}

impl IncomingChangeset {
    #[inline]
    pub fn new(collection: CollectionName, timestamp: ServerTimestamp) -> Self {
        Self::new_with_changes(collection, timestamp, Vec::new())
    }

    #[inline]
    pub fn new_with_changes(
        collection: CollectionName,
        timestamp: ServerTimestamp,
        changes: Vec<IncomingBso>,
    ) -> Self {
        Self {
            changes,
            timestamp,
            collection,
        }
    }
}

#[derive(Debug)]
pub struct OutgoingChangeset {
    pub changes: Vec<OutgoingBso>,
    pub collection: CollectionName,
}

impl OutgoingChangeset {
    #[inline]
    pub fn new(collection: CollectionName, changes: Vec<OutgoingBso>) -> Self {
        Self {
            collection,
            changes,
        }
    }
}

/// A LegacySyncEngine is a deprecated trait that reflects how we used to
/// perform a sync. It can be thought of an an "adaptor" between old engines
/// and the new world order.
///
/// We should try and kill this by moving all engines to the new trait. The priority
/// should be engines which stage incoming and outgoing records to a database to
/// avoid holding the records in memory.
pub trait LegacySyncEngine {
    fn collection_name(&self) -> CollectionName;
    fn prepare_for_sync(&self, _get_client_data: &dyn Fn() -> ClientData) -> Result<()> {
        Ok(())
    }
    fn set_local_encryption_key(&mut self, _key: &str) -> Result<()> {
        unimplemented!("This engine does not support local encryption");
    }
    fn apply_incoming(
        &self,
        inbound: Vec<IncomingChangeset>,
        telem: &mut telemetry::Engine,
    ) -> Result<OutgoingChangeset>;
    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: Vec<Guid>,
    ) -> Result<()>;
    fn get_collection_requests(
        &self,
        server_timestamp: ServerTimestamp,
    ) -> Result<Vec<CollectionRequest>>;
    fn get_sync_assoc(&self) -> Result<EngineSyncAssociation>;
    fn reset(&self, assoc: &EngineSyncAssociation) -> Result<()>;
    fn wipe(&self) -> Result<()>;

    // the magic
    fn get_legacy_engine_state(&self) -> &LegacySyncEngineState;
}

// A "legacy" engine stages incoming and outgoing records in memory via this struct.
#[derive(Debug, Default)]
pub struct LegacySyncEngineState {
    pub staged_incoming: RefCell<Vec<IncomingBso>>,
    pub applied_guids: RefCell<Vec<Guid>>,
    pub last_timestamp: Cell<ServerTimestamp>,
}

impl LegacySyncEngineState {
    fn clear(&self) {
        self.staged_incoming.borrow_mut().clear();
    }
}

impl<T: LegacySyncEngine> SyncEngine for T {
    fn collection_name(&self) -> CollectionName {
        self.collection_name()
    }

    fn prepare_for_sync(&self, get_client_data: &dyn Fn() -> ClientData) -> Result<()> {
        self.get_legacy_engine_state().clear();
        self.prepare_for_sync(get_client_data)
    }

    fn set_local_encryption_key(&mut self, key: &str) -> Result<()> {
        self.set_local_encryption_key(key)
    }

    fn stage_incoming(
        &self,
        mut inbound: Vec<IncomingBso>,
        _telem: &mut telemetry::Engine,
    ) -> Result<()> {
        let state = self.get_legacy_engine_state();
        state.staged_incoming.borrow_mut().append(&mut inbound);
        Ok(())
    }

    /// Apply the staged records.
    fn apply(
        &self,
        timestamp: ServerTimestamp,
        telem: &mut telemetry::Engine,
    ) -> Result<Vec<OutgoingBso>> {
        let state = self.get_legacy_engine_state();
        let incoming_bsos = (*state.staged_incoming.borrow_mut()).drain(..).collect();
        let incoming =
            IncomingChangeset::new_with_changes(self.collection_name(), timestamp, incoming_bsos);
        let outgoing = self.apply_incoming(vec![incoming], telem)?;
        Ok(outgoing.changes)
    }

    fn set_uploaded(&self, new_timestamp: ServerTimestamp, mut ids: Vec<Guid>) -> Result<()> {
        let state = self.get_legacy_engine_state();
        state.applied_guids.borrow_mut().append(&mut ids);
        state.last_timestamp.replace(new_timestamp);
        Ok(())
    }

    /// Called once the sync is finished (ie, after the outgoing records were uploaded to the
    /// server.
    fn sync_finished(&self) -> Result<()> {
        let state = self.get_legacy_engine_state();
        let records_synced = (*state.applied_guids.borrow_mut()).drain(..).collect();
        self.sync_finished(state.last_timestamp.get(), records_synced)
    }

    fn get_collection_request(
        &self,
        server_timestamp: ServerTimestamp,
    ) -> Result<Option<CollectionRequest>> {
        let requests = self.get_collection_requests(server_timestamp)?;
        if requests.len() > 1 {
            panic!("all impls have exactly 0 or 1");
        }
        Ok(requests.into_iter().next())
    }

    fn get_sync_assoc(&self) -> Result<EngineSyncAssociation> {
        self.get_sync_assoc()
    }

    fn reset(&self, assoc: &EngineSyncAssociation) -> Result<()> {
        self.reset(assoc)
    }

    fn wipe(&self) -> Result<()> {
        self.wipe()
    }
}

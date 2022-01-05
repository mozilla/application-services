/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::{MutexPlacesDb, PlacesDb};
use crate::error::*;
use crate::storage::history::{delete_everything, history_sync::reset};
use crate::storage::{get_meta, put_meta};
use interrupt_support::InterruptScope;
use std::sync::Arc;
use sync15::telemetry;
use sync15::{
    CollSyncIds, CollectionRequest, EngineSyncAssociation, IncomingChangeset, OutgoingChangeset,
    ServerTimestamp, SyncEngine,
};
use sync_guid::Guid;

use super::plan::{apply_plan, finish_plan};
use super::MAX_INCOMING_PLACES;

pub const LAST_SYNC_META_KEY: &str = "history_last_sync_time";
// Note that all engines in this crate should use a *different* meta key
// for the global sync ID, because engines are reset individually.
pub const GLOBAL_SYNCID_META_KEY: &str = "history_global_sync_id";
pub const COLLECTION_SYNCID_META_KEY: &str = "history_sync_id";

fn do_apply_incoming(
    db: &PlacesDb,
    interrupt_scope: &InterruptScope,
    inbound: IncomingChangeset,
    telem: &mut telemetry::Engine,
) -> Result<OutgoingChangeset> {
    let timestamp = inbound.timestamp;
    let outgoing = {
        let mut incoming_telemetry = telemetry::EngineIncoming::new();
        let result = apply_plan(db, inbound, &mut incoming_telemetry, interrupt_scope);
        telem.incoming(incoming_telemetry);
        result
    }?;
    // write the timestamp now, so if we are interrupted creating outgoing
    // changesets we don't need to re-reconcile what we just did.
    put_meta(db, LAST_SYNC_META_KEY, &(timestamp.as_millis() as i64))?;
    Ok(outgoing)
}

fn do_sync_finished(
    db: &PlacesDb,
    new_timestamp: ServerTimestamp,
    records_synced: Vec<Guid>,
) -> Result<()> {
    log::info!(
        "sync completed after uploading {} records",
        records_synced.len()
    );
    finish_plan(db)?;

    // write timestamp to reflect what we just wrote.
    put_meta(db, LAST_SYNC_META_KEY, &(new_timestamp.as_millis() as i64))?;

    db.pragma_update(None, "wal_checkpoint", &"PASSIVE")?;

    Ok(())
}

// Short-lived struct that's constructed each sync
pub struct HistorySyncEngine {
    pub db: Arc<MutexPlacesDb>,
    // Public because we use it in the [PlacesApi] sync methods.  We can probably make this private
    // once all syncing goes through the sync manager.
    pub(crate) interrupt_scope: InterruptScope,
}

impl HistorySyncEngine {
    pub fn new(db: Arc<MutexPlacesDb>, interrupt_scope: InterruptScope) -> Self {
        Self {
            db,
            interrupt_scope,
        }
    }
}

impl SyncEngine for HistorySyncEngine {
    fn collection_name(&self) -> std::borrow::Cow<'static, str> {
        "history".into()
    }

    fn apply_incoming(
        &self,
        inbound: Vec<IncomingChangeset>,
        telem: &mut telemetry::Engine,
    ) -> anyhow::Result<OutgoingChangeset> {
        assert_eq!(inbound.len(), 1, "history only requests one item");
        let inbound = inbound.into_iter().next().unwrap();
        let conn = self.db.lock();
        Ok(do_apply_incoming(
            &conn,
            &self.interrupt_scope,
            inbound,
            telem,
        )?)
    }

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: Vec<Guid>,
    ) -> anyhow::Result<()> {
        do_sync_finished(&self.db.lock(), new_timestamp, records_synced)?;
        Ok(())
    }

    fn get_collection_requests(
        &self,
        server_timestamp: ServerTimestamp,
    ) -> anyhow::Result<Vec<CollectionRequest>> {
        let conn = self.db.lock();
        let since =
            ServerTimestamp(get_meta::<i64>(&conn, LAST_SYNC_META_KEY)?.unwrap_or_default());
        Ok(if since == server_timestamp {
            vec![]
        } else {
            vec![CollectionRequest::new("history")
                .full()
                .newer_than(since)
                .limit(MAX_INCOMING_PLACES)]
        })
    }

    fn get_sync_assoc(&self) -> anyhow::Result<EngineSyncAssociation> {
        let conn = self.db.lock();
        let global = get_meta(&conn, GLOBAL_SYNCID_META_KEY)?;
        let coll = get_meta(&conn, COLLECTION_SYNCID_META_KEY)?;
        Ok(if let (Some(global), Some(coll)) = (global, coll) {
            EngineSyncAssociation::Connected(CollSyncIds { global, coll })
        } else {
            EngineSyncAssociation::Disconnected
        })
    }

    fn reset(&self, assoc: &EngineSyncAssociation) -> anyhow::Result<()> {
        reset(&self.db.lock(), assoc)?;
        Ok(())
    }

    fn wipe(&self) -> anyhow::Result<()> {
        delete_everything(&self.db.lock())?;
        Ok(())
    }
}

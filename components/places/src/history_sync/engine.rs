/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::{PlacesDb, SharedPlacesDb};
use crate::error::*;
use crate::storage::history::{delete_everything, history_sync::reset};
use crate::storage::{get_meta, put_meta};
use interrupt_support::SqlInterruptScope;
use std::sync::Arc;
use sync15::bso::{IncomingBso, OutgoingBso};
use sync15::engine::{
    CollSyncIds, CollectionRequest, EngineSyncAssociation, RequestOrder, SyncEngine,
};
use sync15::{telemetry, Guid, ServerTimestamp};

use super::plan::{apply_plan, finish_plan, get_planned_outgoing};
use super::MAX_INCOMING_PLACES;

pub const LAST_SYNC_META_KEY: &str = "history_last_sync_time";
// Note that all engines in this crate should use a *different* meta key
// for the global sync ID, because engines are reset individually.
pub const GLOBAL_SYNCID_META_KEY: &str = "history_global_sync_id";
pub const COLLECTION_SYNCID_META_KEY: &str = "history_sync_id";

fn do_apply_incoming(
    db: &PlacesDb,
    scope: &SqlInterruptScope,
    inbound: Vec<IncomingBso>,
    telem: &mut telemetry::Engine,
) -> Result<()> {
    let mut incoming_telemetry = telemetry::EngineIncoming::new();
    apply_plan(db, inbound, &mut incoming_telemetry, scope)?;
    telem.incoming(incoming_telemetry);
    Ok(())
}

fn do_sync_finished(
    db: &PlacesDb,
    new_timestamp: ServerTimestamp,
    records_synced: Vec<Guid>,
) -> Result<()> {
    info!(
        "sync completed after uploading {} records",
        records_synced.len()
    );
    finish_plan(db)?;

    // write timestamp to reflect what we just wrote.
    // XXX - should clean up transactions, but we *are not* in a transaction
    // here, so this value applies immediately.
    put_meta(db, LAST_SYNC_META_KEY, &new_timestamp.as_millis())?;

    db.pragma_update(None, "wal_checkpoint", "PASSIVE")?;

    Ok(())
}

// Short-lived struct that's constructed each sync
pub struct HistorySyncEngine {
    pub db: Arc<SharedPlacesDb>,
    // We should stage these in a temp table! For now though we just hold them
    // in memory.
    // Public because we use it in the [PlacesApi] sync methods.  We can probably make this private
    // once all syncing goes through the sync manager.
    pub(crate) scope: SqlInterruptScope,
}

impl HistorySyncEngine {
    pub fn new(db: Arc<SharedPlacesDb>) -> Result<Self> {
        Ok(Self {
            scope: db.begin_interrupt_scope()?,
            db,
        })
    }
}

impl SyncEngine for HistorySyncEngine {
    fn collection_name(&self) -> std::borrow::Cow<'static, str> {
        "history".into()
    }

    fn stage_incoming(
        &self,
        inbound: Vec<IncomingBso>,
        telem: &mut telemetry::Engine,
    ) -> anyhow::Result<()> {
        // This is minor abuse of the engine concept, but for each "stage_incoming" call we
        // just apply it directly. We can't advance our timestamp, which means if we are
        // interrupted we'll re-download and re-apply them, but that will be fine in practice.
        let conn = self.db.lock();
        do_apply_incoming(&conn, &self.scope, inbound, telem)?;
        Ok(())
    }

    fn apply(
        &self,
        timestamp: ServerTimestamp,
        _telem: &mut telemetry::Engine,
    ) -> anyhow::Result<Vec<OutgoingBso>> {
        let conn = self.db.lock();
        // We know we've seen everything incoming, so it's safe to write the timestamp now.
        // If we are interrupted creating outgoing BSOs we won't re-apply what we just did.
        put_meta(&conn, LAST_SYNC_META_KEY, &timestamp.as_millis())?;
        Ok(get_planned_outgoing(&conn)?)
    }

    fn set_uploaded(&self, new_timestamp: ServerTimestamp, ids: Vec<Guid>) -> anyhow::Result<()> {
        Ok(do_sync_finished(&self.db.lock(), new_timestamp, ids)?)
    }

    fn sync_finished(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_collection_request(
        &self,
        server_timestamp: ServerTimestamp,
    ) -> anyhow::Result<Option<CollectionRequest>> {
        let conn = self.db.lock();
        let since =
            ServerTimestamp(get_meta::<i64>(&conn, LAST_SYNC_META_KEY)?.unwrap_or_default());
        Ok(if since == server_timestamp {
            None
        } else {
            Some(
                CollectionRequest::new("history".into())
                    .full()
                    .newer_than(since)
                    .limit(MAX_INCOMING_PLACES, RequestOrder::Newest),
            )
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

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::test::new_mem_api;
    use crate::storage::{fetch_page_info, history::delete_visits_for};
    use types::Timestamp;
    use url::Url;

    // If a sync applies a record but stops before `apply()` (eg, it was
    // interrupted), last-sync doesn't advance and the next sync downloads the
    // same record again. A page the user deleted in between must stay deleted,
    // and its tombstone must still be uploaded.
    #[test]
    fn test_reapplied_record_does_not_resurrect_deleted_page() {
        let api = new_mem_api();
        let engine = HistorySyncEngine::new(api.get_sync_connection().unwrap()).unwrap();
        let guid = Guid::new("historyAAA01");
        let url = Url::parse("https://example.com/deleted").unwrap();
        let record = || {
            vec![IncomingBso::from_test_content(serde_json::json!({
                "id": guid,
                "histUri": url.as_str(),
                "title": "deleted page",
                // Microseconds; 1 == "link".
                "visits": [{ "date": 1_700_000_000_000_000u64, "type": 1 }],
            }))]
        };
        let page_exists = || {
            let conn = api.get_sync_connection().unwrap();
            let db = conn.lock();
            fetch_page_info(&db, &url).unwrap().is_some()
        };
        let has_tombstone = || -> bool {
            let conn = api.get_sync_connection().unwrap();
            let db = conn.lock();
            db.query_row(
                "SELECT EXISTS(SELECT 1 FROM moz_places_tombstones WHERE guid = ?)",
                [guid.as_str()],
                |row| row.get(0),
            )
            .unwrap()
        };
        let mut telem = telemetry::Engine::new("history");

        // A sync applies the record, then stops before `apply()`, so last-sync
        // isn't advanced.
        engine.stage_incoming(record(), &mut telem).unwrap();
        assert!(page_exists());

        // The user deletes the page. It came from sync and isn't bookmarked, so
        // the page row goes and a page tombstone is written.
        {
            let conn = api.get_sync_connection().unwrap();
            delete_visits_for(&conn.lock(), &guid).unwrap();
        }
        assert!(!page_exists());
        assert!(has_tombstone(), "sanity check: deleting wrote a tombstone");

        // The next sync downloads the same, unchanged record again.
        engine.stage_incoming(record(), &mut telem).unwrap();

        assert!(!page_exists(), "the deleted page came back");
        assert!(has_tombstone(), "the deletion will never be uploaded");

        // And the deletion is what gets uploaded.
        let outgoing = engine.apply(ServerTimestamp(0), &mut telem).unwrap();
        assert!(
            outgoing.iter().any(|bso| bso.envelope.id == guid),
            "the page's tombstone should be uploaded"
        );
    }

    // The other half of desktop's rule: if the page changed on the server
    // after we deleted it locally, the server's copy wins and the page is back.
    #[test]
    fn test_newer_remote_record_restores_deleted_page() {
        let api = new_mem_api();
        let engine = HistorySyncEngine::new(api.get_sync_connection().unwrap()).unwrap();
        let guid = Guid::new("historyAAA02");
        let url = Url::parse("https://example.com/restored").unwrap();
        let record = |modified: ServerTimestamp| {
            vec![IncomingBso::from_test_content_ts(
                serde_json::json!({
                    "id": guid,
                    "histUri": url.as_str(),
                    "title": "restored page",
                    // Microseconds; 1 == "link".
                    "visits": [{ "date": 1_700_000_000_000_000u64, "type": 1 }],
                }),
                modified,
            )]
        };
        let mut telem = telemetry::Engine::new("history");

        engine
            .stage_incoming(record(ServerTimestamp(0)), &mut telem)
            .unwrap();
        {
            let conn = api.get_sync_connection().unwrap();
            delete_visits_for(&conn.lock(), &guid).unwrap();
        }

        // Another device visits the page after we deleted it.
        let later = ServerTimestamp::from_millis(Timestamp::now().as_millis() as i64 + 60_000);
        engine.stage_incoming(record(later), &mut telem).unwrap();

        let conn = api.get_sync_connection().unwrap();
        let db = conn.lock();
        assert!(
            fetch_page_info(&db, &url).unwrap().is_some(),
            "the page should be back"
        );
        let has_tombstone: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM moz_places_tombstones WHERE guid = ?)",
                [guid.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!has_tombstone, "the stale deletion should be dropped");
    }
}

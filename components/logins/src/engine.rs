/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::CLONE_ENTIRE_MIRROR_SQL;
use crate::error::*;
use crate::login::{LocalLogin, Login, MirrorLogin, SyncLoginData, SyncStatus};
use crate::schema;
use crate::update_plan::UpdatePlan;
use crate::LoginDb;
use crate::LoginStore;
use rusqlite::NO_PARAMS;
use sql_support::SqlInterruptScope;
use sql_support::{self, ConnExt};
use std::collections::HashSet;
use std::sync::Arc;
use sync15::{
    telemetry, CollSyncIds, CollectionRequest, EngineSyncAssociation, IncomingChangeset,
    OutgoingChangeset, Payload, ServerTimestamp, SyncEngine,
};
use sync_guid::Guid;

// The sync engine.
pub struct LoginsSyncEngine {
    pub store: Arc<LoginStore>,
    pub scope: sql_support::SqlInterruptScope,
}

impl LoginsSyncEngine {
    pub fn new(store: Arc<LoginStore>) -> Self {
        let scope = store.db.lock().unwrap().begin_interrupt_scope();
        Self { store, scope }
    }

    fn reconcile(
        &self,
        records: Vec<SyncLoginData>,
        server_now: ServerTimestamp,
        telem: &mut telemetry::EngineIncoming,
        scope: &SqlInterruptScope,
    ) -> Result<UpdatePlan> {
        let mut plan = UpdatePlan::default();

        for mut record in records {
            scope.err_if_interrupted()?;
            log::debug!("Processing remote change {}", record.guid());
            let upstream = if let Some(inbound) = record.inbound.0.take() {
                inbound
            } else {
                log::debug!("Processing inbound deletion (always prefer)");
                plan.plan_delete(record.guid.clone());
                continue;
            };
            let upstream_time = record.inbound.1;
            match (record.mirror.take(), record.local.take()) {
                (Some(mirror), Some(local)) => {
                    log::debug!("  Conflict between remote and local, Resolving with 3WM");
                    plan.plan_three_way_merge(local, mirror, upstream, upstream_time, server_now);
                    telem.reconciled(1);
                }
                (Some(_mirror), None) => {
                    log::debug!("  Forwarding mirror to remote");
                    plan.plan_mirror_update(upstream, upstream_time);
                    telem.applied(1);
                }
                (None, Some(local)) => {
                    log::debug!("  Conflicting record without shared parent, using newer");
                    plan.plan_two_way_merge(&local.login, (upstream, upstream_time));
                    telem.reconciled(1);
                }
                (None, None) => {
                    if let Some(dupe) = self.store.db.lock().unwrap().find_dupe(&upstream)? {
                        log::debug!(
                            "  Incoming record {} was is a dupe of local record {}",
                            upstream.guid(),
                            dupe.guid()
                        );
                        plan.plan_two_way_merge(&dupe, (upstream, upstream_time));
                    } else {
                        log::debug!("  No dupe found, inserting into mirror");
                        plan.plan_mirror_insert(upstream, upstream_time, false);
                    }
                    telem.applied(1);
                }
            }
        }
        Ok(plan)
    }

    fn execute_plan(&self, plan: UpdatePlan, scope: &SqlInterruptScope) -> Result<()> {
        // Because rusqlite want a mutable reference to create a transaction
        // (as a way to save us from ourselves), we side-step that by creating
        // it manually.
        let db = self.store.db.lock().unwrap();
        let tx = db.unchecked_transaction()?;
        plan.execute(&tx, scope)?;
        tx.commit()?;
        Ok(())
    }

    // Fetch all the data for the provided IDs.
    // TODO: Might be better taking a fn instead of returning all of it... But that func will likely
    // want to insert stuff while we're doing this so ugh.
    fn fetch_login_data(
        &self,
        records: &[(sync15::Payload, ServerTimestamp)],
        telem: &mut telemetry::EngineIncoming,
        scope: &SqlInterruptScope,
    ) -> Result<Vec<SyncLoginData>> {
        let mut sync_data = Vec::with_capacity(records.len());
        {
            let mut seen_ids: HashSet<Guid> = HashSet::with_capacity(records.len());
            for incoming in records.iter() {
                if seen_ids.contains(&incoming.0.id) {
                    throw!(ErrorKind::DuplicateGuid(incoming.0.id.to_string()))
                }
                seen_ids.insert(incoming.0.id.clone());
                match SyncLoginData::from_payload(incoming.0.clone(), incoming.1) {
                    Ok(v) => sync_data.push(v),
                    Err(e) => {
                        log::error!("Failed to deserialize record {:?}: {}", incoming.0.id, e);
                        // Ideally we'd track new_failed, but it's unclear how
                        // much value it has.
                        telem.failed(1);
                    }
                }
            }
        }
        scope.err_if_interrupted()?;

        sql_support::each_chunk_mapped(
            &records,
            |r| r.0.id.as_str(),
            |chunk, offset| -> Result<()> {
                // pairs the bound parameter for the guid with an integer index.
                let values_with_idx = sql_support::repeat_display(chunk.len(), ",", |i, f| {
                    write!(f, "({},?)", i + offset)
                });
                let query = format!(
                    "WITH to_fetch(guid_idx, fetch_guid) AS (VALUES {vals})
                     SELECT
                         {common_cols},
                         is_overridden,
                         server_modified,
                         NULL as local_modified,
                         NULL as is_deleted,
                         NULL as sync_status,
                         1 as is_mirror,
                         to_fetch.guid_idx as guid_idx
                     FROM loginsM
                     JOIN to_fetch
                         ON loginsM.guid = to_fetch.fetch_guid

                     UNION ALL

                     SELECT
                         {common_cols},
                         NULL as is_overridden,
                         NULL as server_modified,
                         local_modified,
                         is_deleted,
                         sync_status,
                         0 as is_mirror,
                         to_fetch.guid_idx as guid_idx
                     FROM loginsL
                     JOIN to_fetch
                         ON loginsL.guid = to_fetch.fetch_guid",
                    // give each VALUES item 2 entries, an index and the parameter.
                    vals = values_with_idx,
                    common_cols = schema::COMMON_COLS,
                );

                let db = &self.store.db.lock().unwrap();
                let mut stmt = db.prepare(&query)?;

                let rows = stmt.query_and_then(chunk, |row| {
                    let guid_idx_i = row.get::<_, i64>("guid_idx")?;
                    // Hitting this means our math is wrong...
                    assert!(guid_idx_i >= 0);

                    let guid_idx = guid_idx_i as usize;
                    let is_mirror: bool = row.get("is_mirror")?;
                    if is_mirror {
                        sync_data[guid_idx].set_mirror(MirrorLogin::from_row(row)?)?;
                    } else {
                        sync_data[guid_idx].set_local(LocalLogin::from_row(row)?)?;
                    }
                    scope.err_if_interrupted()?;
                    Ok(())
                })?;
                // `rows` is an Iterator<Item = Result<()>>, so we need to collect to handle the errors.
                rows.collect::<Result<_>>()?;
                Ok(())
            },
        )?;
        Ok(sync_data)
    }

    fn fetch_outgoing(
        &self,
        st: ServerTimestamp,
        scope: &SqlInterruptScope,
    ) -> Result<OutgoingChangeset> {
        // Taken from iOS. Arbitrarily large, so that clients that want to
        // process deletions first can; for us it doesn't matter.
        const TOMBSTONE_SORTINDEX: i32 = 5_000_000;
        const DEFAULT_SORTINDEX: i32 = 1;
        let mut outgoing = OutgoingChangeset::new("passwords", st);
        let db = self.store.db.lock().unwrap();
        let mut stmt = db.prepare_cached(&format!(
            "SELECT * FROM loginsL WHERE sync_status IS NOT {synced}",
            synced = SyncStatus::Synced as u8
        ))?;
        let rows = stmt.query_and_then(NO_PARAMS, |row| {
            scope.err_if_interrupted()?;
            Ok(if row.get::<_, bool>("is_deleted")? {
                Payload::new_tombstone(row.get::<_, String>("guid")?)
                    .with_sortindex(TOMBSTONE_SORTINDEX)
            } else {
                let login = Login::from_row(row)?;
                Payload::from_record(login)?.with_sortindex(DEFAULT_SORTINDEX)
            })
        })?;
        outgoing.changes = rows.collect::<Result<_>>()?;

        Ok(outgoing)
    }

    fn do_apply_incoming(
        &self,
        inbound: IncomingChangeset,
        telem: &mut telemetry::Engine,
        scope: &SqlInterruptScope,
    ) -> Result<OutgoingChangeset> {
        let mut incoming_telemetry = telemetry::EngineIncoming::new();
        let data = self.fetch_login_data(&inbound.changes, &mut incoming_telemetry, scope)?;
        let plan = {
            let result = self.reconcile(data, inbound.timestamp, &mut incoming_telemetry, scope);
            telem.incoming(incoming_telemetry);
            result
        }?;
        self.execute_plan(plan, scope)?;
        self.fetch_outgoing(inbound.timestamp, scope)
    }

    fn set_last_sync(&self, db: &LoginDb, last_sync: ServerTimestamp) -> Result<()> {
        log::debug!("Updating last sync to {}", last_sync);
        let last_sync_millis = last_sync.as_millis() as i64;
        db.put_meta(schema::LAST_SYNC_META_KEY, &last_sync_millis)
    }

    fn get_last_sync(&self, db: &LoginDb) -> Result<Option<ServerTimestamp>> {
        let millis = db.get_meta::<i64>(schema::LAST_SYNC_META_KEY)?.unwrap();
        Ok(Some(ServerTimestamp(millis)))
    }

    pub fn set_global_state(&self, state: &Option<String>) -> Result<()> {
        let to_write = match state {
            Some(ref s) => s,
            None => "",
        };
        let db = self.store.db.lock().unwrap();
        db.put_meta(schema::GLOBAL_STATE_META_KEY, &to_write)
    }

    pub fn get_global_state(&self) -> Result<Option<String>> {
        let db = self.store.db.lock().unwrap();
        db.get_meta::<String>(schema::GLOBAL_STATE_META_KEY)
    }

    fn mark_as_synchronized(
        &self,
        guids: &[&str],
        ts: ServerTimestamp,
        scope: &SqlInterruptScope,
    ) -> Result<()> {
        let db = self.store.db.lock().unwrap();
        let tx = db.unchecked_transaction()?;
        sql_support::each_chunk(guids, |chunk, _| -> Result<()> {
            db.execute(
                &format!(
                    "DELETE FROM loginsM WHERE guid IN ({vars})",
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                chunk,
            )?;
            scope.err_if_interrupted()?;

            db.execute(
                &format!(
                    "INSERT OR IGNORE INTO loginsM (
                         {common_cols}, is_overridden, server_modified
                     )
                     SELECT {common_cols}, 0, {modified_ms_i64}
                     FROM loginsL
                     WHERE is_deleted = 0 AND guid IN ({vars})",
                    common_cols = schema::COMMON_COLS,
                    modified_ms_i64 = ts.as_millis() as i64,
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                chunk,
            )?;
            scope.err_if_interrupted()?;

            db.execute(
                &format!(
                    "DELETE FROM loginsL WHERE guid IN ({vars})",
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                chunk,
            )?;
            scope.err_if_interrupted()?;
            Ok(())
        })?;
        self.set_last_sync(&db, ts)?;
        tx.commit()?;
        Ok(())
    }

    // This exists here as a public function so the store can call it. Ideally
    // the store would not do that :) Then it can go back into the sync trait
    // and return an anyhow::Result
    pub fn do_reset(&self, assoc: &EngineSyncAssociation) -> Result<()> {
        log::info!("Executing reset on password engine!");
        let db = self.store.db.lock().unwrap();
        let tx = db.unchecked_transaction()?;
        db.execute_all(&[
            &CLONE_ENTIRE_MIRROR_SQL,
            "DELETE FROM loginsM",
            &format!("UPDATE loginsL SET sync_status = {}", SyncStatus::New as u8),
        ])?;
        self.set_last_sync(&db, ServerTimestamp(0))?;
        match assoc {
            EngineSyncAssociation::Disconnected => {
                db.delete_meta(schema::GLOBAL_SYNCID_META_KEY)?;
                db.delete_meta(schema::COLLECTION_SYNCID_META_KEY)?;
            }
            EngineSyncAssociation::Connected(ids) => {
                db.put_meta(schema::GLOBAL_SYNCID_META_KEY, &ids.global)?;
                db.put_meta(schema::COLLECTION_SYNCID_META_KEY, &ids.coll)?;
            }
        };
        db.delete_meta(schema::GLOBAL_STATE_META_KEY)?;
        tx.commit()?;
        Ok(())
    }
}

impl SyncEngine for LoginsSyncEngine {
    fn collection_name(&self) -> std::borrow::Cow<'static, str> {
        "passwords".into()
    }

    fn apply_incoming(
        &self,
        inbound: Vec<IncomingChangeset>,
        telem: &mut telemetry::Engine,
    ) -> anyhow::Result<OutgoingChangeset> {
        assert_eq!(inbound.len(), 1, "logins only requests one item");
        let inbound = inbound.into_iter().next().unwrap();
        Ok(self.do_apply_incoming(inbound, telem, &self.scope)?)
    }

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: Vec<Guid>,
    ) -> anyhow::Result<()> {
        self.mark_as_synchronized(
            &records_synced.iter().map(Guid::as_str).collect::<Vec<_>>(),
            new_timestamp,
            &self.scope,
        )?;
        Ok(())
    }

    fn get_collection_requests(
        &self,
        server_timestamp: ServerTimestamp,
    ) -> anyhow::Result<Vec<CollectionRequest>> {
        let db = self.store.db.lock().unwrap();
        let since = self.get_last_sync(&db)?.unwrap_or_default();
        Ok(if since == server_timestamp {
            vec![]
        } else {
            vec![CollectionRequest::new("passwords").full().newer_than(since)]
        })
    }

    fn get_sync_assoc(&self) -> anyhow::Result<EngineSyncAssociation> {
        let db = self.store.db.lock().unwrap();
        let global = db.get_meta(schema::GLOBAL_SYNCID_META_KEY)?;
        let coll = db.get_meta(schema::COLLECTION_SYNCID_META_KEY)?;
        Ok(if let (Some(global), Some(coll)) = (global, coll) {
            EngineSyncAssociation::Connected(CollSyncIds { global, coll })
        } else {
            EngineSyncAssociation::Disconnected
        })
    }

    fn reset(&self, assoc: &EngineSyncAssociation) -> anyhow::Result<()> {
        self.do_reset(assoc)?;
        Ok(())
    }

    fn wipe(&self) -> anyhow::Result<()> {
        let db = self.store.db.lock().unwrap();
        db.wipe(&self.scope)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::LoginStore;
    #[test]
    fn test_bad_record() {
        let store = Arc::new(LoginStore::new_in_memory(Some("testing")).unwrap());
        let engine = LoginsSyncEngine::new(Arc::clone(&store));
        let mut telem = sync15::telemetry::EngineIncoming::new();
        let res = engine
            .fetch_login_data(
                &[
                    // tombstone
                    (
                        sync15::Payload::new_tombstone("dummy_000001"),
                        sync15::ServerTimestamp(10000),
                    ),
                    // invalid
                    (
                        sync15::Payload::from_json(serde_json::json!({
                            "id": "dummy_000002",
                            "garbage": "data",
                            "etc": "not a login"
                        }))
                        .unwrap(),
                        sync15::ServerTimestamp(10000),
                    ),
                    // valid
                    (
                        sync15::Payload::from_json(serde_json::json!({
                            "id": "dummy_000003",
                            "formSubmitURL": "https://www.example.com/submit",
                            "hostname": "https://www.example.com",
                            "username": "test",
                            "password": "test",
                        }))
                        .unwrap(),
                        sync15::ServerTimestamp(10000),
                    ),
                ],
                &mut telem,
                &engine.scope,
            )
            .unwrap();
        assert_eq!(telem.get_failed(), 1);
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].guid, "dummy_000001");
        assert_eq!(res[1].guid, "dummy_000003");
    }
}

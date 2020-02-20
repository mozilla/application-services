/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::upgrades::UpgradeKind;
use super::{bundle::ToLocalReason, meta, LocalRecord, NativeRecord, SchemaBundle, SyncStatus};
use crate::error::*;
use crate::ms_time::MsTime;
use crate::sync::records as syncing;
use crate::sync::records::*;
use crate::sync::schema_action::UpgradeRemote;
use crate::vclock::{Counter, VClock};
use crate::Guid;
use crate::RecordSchema;
use rusqlite::{named_params, Connection};
use sql_support::{ConnExt, SqlInterruptHandle, SqlInterruptScope};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::{atomic::AtomicUsize, Arc, Mutex};
use sync15_traits::ServerTimestamp;
pub struct RemergeDb {
    db: Connection,
    info: SchemaBundle,
    client_id: sync_guid::Guid,
    interrupt_counter: Arc<AtomicUsize>,
}

lazy_static::lazy_static! {
    // XXX: We should replace this with something like the PlacesApi path-based
    // hashmap, but for now this is better than nothing.
    static ref DB_INIT_MUTEX: Mutex<()> = Mutex::new(());
}

impl RemergeDb {
    pub(crate) fn with_connection(mut db: Connection, native: Arc<RecordSchema>) -> Result<Self> {
        let _g = DB_INIT_MUTEX.lock().unwrap();
        let pragmas = "
            -- The value we use was taken from Desktop Firefox, and seems necessary to
            -- help ensure good performance. The default value is 1024, which the SQLite
            -- docs themselves say is too small and should be changed.
            PRAGMA page_size = 32768;

            -- Disable calling mlock/munlock for every malloc/free.
            -- In practice this results in a massive speedup, especially
            -- for insert-heavy workloads.
            PRAGMA cipher_memory_security = false;

            -- `temp_store = 2` is required on Android to force the DB to keep temp
            -- files in memory, since on Android there's no tmp partition. See
            -- https://github.com/mozilla/mentat/issues/505. Ideally we'd only
            -- do this on Android, and/or allow caller to configure it.
            -- (although see also bug 1313021, where Firefox enabled it for both
            -- Android and 64bit desktop builds)
            PRAGMA temp_store = 2;

            -- We want foreign-key support.
            PRAGMA foreign_keys = ON;

            -- we unconditionally want write-ahead-logging mode
            PRAGMA journal_mode=WAL;

            -- How often to autocheckpoint (in units of pages).
            -- 2048000 (our max desired WAL size) / 32768 (page size).
            PRAGMA wal_autocheckpoint=62
        ";
        db.execute_batch(pragmas)?;
        let tx = db.transaction()?;
        super::schema::init(&tx)?;
        let (info, client_id) = super::bootstrap::load_or_bootstrap(&tx, native)?;
        tx.commit()?;
        Ok(RemergeDb {
            db,
            info,
            client_id,
            interrupt_counter: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub(crate) fn conn(&self) -> &rusqlite::Connection {
        &self.db
    }
    pub fn collection(&self) -> &str {
        &self.info.collection_name
    }
    pub fn info(&self) -> &SchemaBundle {
        &self.info
    }

    pub fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.db.query_row_named(
            "SELECT EXISTS(
                 SELECT 1 FROM rec_local
                 WHERE guid = :guid AND is_deleted = 0
                 UNION ALL
                 SELECT 1 FROM rec_mirror
                 WHERE guid = :guid AND is_overridden IS NOT 1
             )",
            named_params! { ":guid": id },
            |row| row.get(0),
        )?)
    }
    pub(crate) fn fetch_for_sync(
        &self,
        records: Vec<(sync15_traits::Payload, sync15_traits::ServerTimestamp)>,
        telem: &mut sync15_traits::telemetry::EngineIncoming,
        scope: &SqlInterruptScope,
    ) -> Result<HashMap<Guid, syncing::RecordInfo>> {
        let mut sync_data: HashMap<Guid, syncing::RecordInfo> =
            HashMap::with_capacity(records.len());
        {
            for (incoming, time) in records.into_iter() {
                let id = incoming.id.clone();
                match incoming.into_record::<syncing::RemoteRecord>() {
                    Ok(rec) => {
                        sync_data.insert(id, syncing::RecordInfo::new(rec, time));
                    }
                    Err(e) => {
                        log::error!("Failed to deserialize record {:?}: {}", id, e);
                        // Ideally we'd track new_failed, but it's unclear how
                        // much value it has.
                        telem.failed(1);
                    }
                }
            }
        }
        scope
            .err_if_interrupted()
            .map_err(|_| ErrorKind::Interrupted)?;
        let just_ids = sync_data.keys().cloned().collect::<Vec<_>>();

        sql_support::each_chunk_mapped(
            &just_ids,
            |id| id.as_str(),
            |chunk, _offset| -> Result<()> {
                let query = format!(
                    "WITH to_fetch(fetch_guid) AS (VALUES {vals})
                     SELECT
                         guid,
                         record_data,
                         is_overridden,
                         server_modified_ms,
                         vector_clock,
                         last_writer_id,
                         remerge_schema_version,
                         NULL as local_modified_ms,
                         is_deleted,
                         -- NULL as sync_status,
                         1 as is_mirror
                     FROM rec_mirror
                     JOIN to_fetch
                         ON rec_mirror.guid = to_fetch.fetch_guid

                     UNION ALL

                     SELECT
                         guid,
                         record_data,
                         NULL as is_overridden,
                         NULL as server_modified_ms,
                         vector_clock,
                         last_writer_id,
                         remerge_schema_version,
                         local_modified_ms,
                         is_deleted,
                         -- sync_status,
                         0 as is_mirror
                     FROM rec_local
                     JOIN to_fetch
                         ON rec_local.guid = to_fetch.fetch_guid",
                    // give each VALUES item 2 entries, an index and the parameter.
                    vals = sql_support::repeat_sql_values(chunk.len())
                );

                let mut stmt = self.db.prepare(&query)?;

                let rows = stmt.query_and_then(chunk, |row| {
                    let guid = row.get::<_, Guid>("guid")?;
                    let is_mirror: bool = row.get("is_mirror")?;
                    let vclock = row.get::<_, VClock>("vector_clock")?;
                    let last_writer = row.get::<_, Guid>("last_writer_id")?;
                    let is_deleted = row.get::<_, bool>("is_deleted")?;
                    let recdata = if is_deleted {
                        let record = row.get::<_, super::RawRecord>("record_data")?;
                        let schema_ver = row.get::<_, String>("remerge_schema_version")?;
                        Some((record, schema_ver))
                    } else {
                        None
                    };
                    let mut targ = &mut sync_data.get_mut(&guid).unwrap();
                    if is_mirror {
                        let server_modified = row.get::<_, i64>("server_modified_ms")?;
                        let is_overridden = row.get::<_, bool>("is_overridden")?;
                        targ.mirror = Some(syncing::MirrorRecord {
                            id: guid,
                            // None if tombstone. String is schema version
                            inner: recdata,
                            server_modified: sync15_traits::ServerTimestamp::from_millis(
                                server_modified,
                            ),
                            vclock,
                            last_writer,
                            is_overridden,
                        });
                    } else {
                        let local_modified = row.get::<_, MsTime>("local_modified_ms")?;
                        targ.local = Some(syncing::LocalRecord {
                            id: guid,
                            inner: recdata,
                            local_modified,
                            vclock,
                            last_writer,
                        });
                    }

                    scope
                        .err_if_interrupted()
                        .map_err(|_| ErrorKind::Interrupted)?;
                    Ok(())
                })?;
                // `rows` is an Iterator<Item = Result<()>>, so we need to collect to handle the errors.
                rows.collect::<Result<_>>()?;
                Ok(())
            },
        )?;
        Ok(sync_data)
    }

    pub(crate) fn sync_delete_mirror(&self, r: Guid) -> Result<()> {
        self.conn()
            .execute("DELETE FROM rec_mirror WHERE guid = ?", &[r])?;
        Ok(())
    }
    pub(crate) fn sync_delete_local(&self, r: Guid) -> Result<()> {
        self.conn()
            .execute("DELETE FROM rec_local WHERE guid = ?", &[r])?;
        Ok(())
    }

    pub(crate) fn sync_delete(&self, r: Guid) -> Result<()> {
        self.sync_delete_local(r.clone())?;
        self.sync_delete_mirror(r)
    }

    pub(crate) fn sync_mirror_update(
        &self,
        rec: RemoteRecord,
        vclock: VClock,
        time: ServerTimestamp,
    ) -> Result<()> {
        self.db.execute_named(
            "UPDATE rec_mirror (
                guid,
                remerge_schema_version,
                record_data,
                server_modified_ms,
                vector_clock,
                last_writer_id
            ) VALUES (
                :guid,
                :schema_ver,
                :record,
                :time,
                :vclock,
                :writer
            )",
            named_params! {
                ":guid": rec.id,
                ":schema_ver": rec.schema_version,
                ":record": rec.payload.unwrap(),
                ":time": time.as_millis(),
                ":vclock": vclock,
                ":writer": rec.last_writer,
            },
        )?;
        Ok(())
    }
    pub(crate) fn mark_synchronized(&mut self, ts: ServerTimestamp, guids: &[Guid]) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        sql_support::each_chunk(&guids, |chunk, _| -> Result<()> {
            if chunk.is_empty() {
                return Ok(());
            }
            self.db.execute(
                &format!(
                    "DELETE FROM rec_mirror WHERE guid IN ({vars})",
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                chunk,
            )?;

            self.db.execute(
                &format!(
                    "INSERT OR IGNORE INTO rec_mirror (
                        guid, remerge_schema_version, record_data, vector_clock, last_writer_id, is_overridden, server_modified_ms
                     )
                     SELECT guid, remerge_schema_version, record_data, vector_clock, last_writer_id, 0, {modified_ms_i64}
                     FROM rec_local
                     WHERE is_deleted = 0 AND guid IN ({vars})",
                    // common_cols = schema::COMMON_COLS,
                    modified_ms_i64 = ts.as_millis() as i64,
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                chunk,
            )?;

            self.db.execute(
                &format!(
                    "DELETE FROM rec_local WHERE guid IN ({vars})",
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                chunk,
            )?;
            // scope.err_if_interrupted()?;
            Ok(())
        })?;
        meta::put(&tx, meta::LAST_SYNC_SERVER_MS, &ts.as_millis())?;
        tx.commit()?;
        Ok(())
    }
    pub fn fetch_outgoing(&self) -> Result<Vec<sync15_traits::Payload>> {
        let mut stmt = self.db.prepare_cached(&format!(
            "SELECT * FROM rec_local WHERE sync_status IS NOT {synced}",
            synced = SyncStatus::Synced as u8
        ))?;
        let rows = stmt.query_and_then(rusqlite::NO_PARAMS, |row| {
            let mut rec = RemoteRecord {
                id: row.get("guid")?,
                vclock: row.get("vector_clock")?,
                last_writer: self.client_id(),
                schema_version: row.get("remerge_schema_version")?,
                deleted: row.get("is_deleted")?,
                payload: row.get("record_data")?,
            };
            if rec.deleted {
                rec.payload = None;
            }
            Ok(sync15_traits::Payload::from_record(rec)?)
        })?;
        rows.collect::<Result<_>>()
    }

    pub(crate) fn sync_mirror_insert(
        &self,
        rec: RemoteRecord,
        vclock: VClock,
        time: ServerTimestamp,
        is_override: bool,
    ) -> Result<()> {
        self.db.execute_named(
            "INSERT OR IGNORE INTO rec_mirror (
                guid,
                remerge_schema_version,
                record_data,
                server_modified_ms,
                is_deleted,
                is_overridden,
                vector_clock,
                last_writer_id
            ) VALUES (
                :guid,
                :schema_ver,
                :record,
                :time,
                0,
                :overridden,
                :vclock,
                :writer
            )",
            named_params! {
                ":guid": rec.id,
                ":schema_ver": rec.schema_version,
                ":record": rec.payload.unwrap(),
                ":time": time.as_millis(),
                ":overridden": is_override,
                ":vclock": vclock,
                ":writer": rec.last_writer,
            },
        )?;
        Ok(())
    }

    pub fn create(&self, native: &NativeRecord) -> Result<Guid> {
        let (id, record) = self
            .info
            .native_to_local(&native, ToLocalReason::Creation)?;
        let tx = self.db.unchecked_transaction()?;
        // TODO: Search DB for dupes based on the value of the fields listed in dedupe_on.
        let id_exists = self.exists(id.as_ref())?;
        if id_exists {
            throw!(InvalidRecord::IdNotUnique);
        }
        if self.dupe_exists(&record)? {
            throw!(InvalidRecord::Duplicate);
        }
        let ctr = self.counter_bump()?;
        let vclock = VClock::new(self.client_id(), ctr);

        let now = MsTime::now();
        self.db.execute_named(
            "INSERT INTO rec_local (
                guid,
                remerge_schema_version,
                record_data,
                local_modified_ms,
                is_deleted,
                sync_status,
                vector_clock,
                last_writer_id
            ) VALUES (
                :guid,
                :schema_ver,
                :record,
                :now,
                0,
                :status,
                :vclock,
                :client_id
            )",
            named_params! {
                ":guid": id,
                ":schema_ver": self.info.local.version.to_string(),
                ":record": record,
                ":now": now,
                ":status": SyncStatus::New as u8,
                ":vclock": vclock,
                ":client_id": self.client_id,
            },
        )?;
        tx.commit()?;
        Ok(id)
    }

    fn counter_bump(&self) -> Result<Counter> {
        let mut ctr = meta::get::<i64>(&self.db, meta::CHANGE_COUNTER)?;
        assert!(
            ctr >= 0,
            "Corrupt db? negative global change counter: {:?}",
            ctr
        );
        ctr += 1;
        meta::put(&self.db, meta::CHANGE_COUNTER, &ctr)?;
        // Overflowing i64 takes around 9 quintillion (!!) writes, so the only
        // way it can realistically happen is on db corruption.
        //
        // FIXME: We should be returning a specific error for DB corruption
        // instead of panicing, and have a maintenance routine (a la places).
        Ok(Counter::try_from(ctr).expect("Corrupt db? i64 overflow"))
    }

    fn get_vclock(&self, id: &str) -> Result<VClock> {
        Ok(self.db.query_row_named(
            "SELECT vector_clock FROM rec_local
             WHERE guid = :guid AND is_deleted = 0
             UNION ALL
             SELECT vector_clock FROM rec_mirror
             WHERE guid = :guid AND is_overridden IS NOT 1",
            named_params! { ":guid": id },
            |row| row.get(0),
        )?)
    }

    pub fn delete_by_id(&self, id: &str) -> Result<bool> {
        let tx = self.db.unchecked_transaction()?;
        let exists = self.exists(id)?;
        if !exists {
            // Hrm, is there anything else we should do here? Logins goes
            // through the whole process (which is tricker for us...)
            return Ok(false);
        }
        let now_ms = MsTime::now();
        let vclock = self.get_bumped_vclock(id)?;

        // Locally, mark is_deleted and clear sensitive fields
        self.db.execute_named(
            "UPDATE rec_local
             SET local_modified_ms = :now_ms,
                 sync_status = :changed,
                 is_deleted = 1,
                 record_data = '{}',
                 vector_clock = :vclock,
                 last_writer_id = :own_id
             WHERE guid = :guid",
            named_params! {
                ":now_ms": now_ms,
                ":changed": SyncStatus::Changed as u8,
                ":guid": id,
                ":vclock": vclock,
                ":own_id": self.client_id,
            },
        )?;

        // Mark the mirror as overridden. XXX should we clear `record_data` here too?
        self.db.execute_named(
            "UPDATE rec_mirror SET is_overridden = 1 WHERE guid = :guid",
            named_params! { ":guid": id },
        )?;

        // If we don't have a local record for this ID, but do have it in the
        // mirror, insert tombstone.
        self.db.execute_named(
            "INSERT OR IGNORE INTO rec_local
                    (guid, local_modified_ms, is_deleted, sync_status, record_data, vector_clock, last_writer_id, remerge_schema_version)
             SELECT guid, :now_ms,           1,          :changed,    '{}',        :vclock,      :own_id,        :schema_ver
             FROM rec_mirror
             WHERE guid = :guid",
            named_params! {
                ":now_ms": now_ms,
                ":guid": id,
                ":schema_ver": self.info.local.version.to_string(),
                ":vclock": vclock,
                ":changed": SyncStatus::Changed as u8,
            })?;
        tx.commit()?;
        Ok(exists)
    }

    fn get_local_by_id(&self, id: &str) -> Result<Option<LocalRecord>> {
        Ok(self.db.try_query_row(
            "SELECT record_data FROM rec_local WHERE guid = :guid AND is_deleted = 0
             UNION ALL
             SELECT record_data FROM rec_mirror WHERE guid = :guid AND is_overridden = 0
             LIMIT 1",
            named_params! { ":guid": id },
            |r| r.get(0),
            true, // cache
        )?)
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<NativeRecord>> {
        self.get_local_by_id(id)?
            .map(|v| self.info.local_to_native(&v))
            .transpose()
    }

    pub fn get_all(&self) -> Result<Vec<NativeRecord>> {
        let mut stmt = self.db.prepare_cached(
            "SELECT record_data FROM rec_local WHERE is_deleted = 0
             UNION ALL
             SELECT record_data FROM rec_mirror WHERE is_overridden = 0",
        )?;
        let rows = stmt.query_and_then(rusqlite::NO_PARAMS, |row| -> Result<NativeRecord> {
            let r: LocalRecord = row.get("record_data")?;
            self.info.local_to_native(&r)
        })?;
        rows.collect::<Result<_>>()
    }

    fn ensure_local_overlay_exists(&self, guid: &str) -> Result<()> {
        let already_have_local: bool = self.db.query_row_named(
            "SELECT EXISTS(SELECT 1 FROM rec_local WHERE guid = :guid)",
            named_params! { ":guid": guid },
            |row| row.get(0),
        )?;

        if already_have_local {
            return Ok(());
        }

        log::debug!("No overlay; cloning one for {:?}.", guid);
        self.clone_mirror_to_overlay(guid)
    }

    // Note: unlike the version of this function in `logins`, we return Err if
    // `guid` is invalid instead of expecting the caller to check
    fn clone_mirror_to_overlay(&self, guid: &str) -> Result<()> {
        let sql = "
            INSERT OR IGNORE INTO rec_local
                (guid, record_data, vector_clock, last_writer_id, local_modified_ms, is_deleted, sync_status)
            SELECT
                 guid, record_data, vector_clock, last_writer_id, 0 as local_modified_ms, 0 AS is_deleted, 0 AS sync_status
            FROM rec_mirror
            WHERE guid = :guid
        ";
        let changed = self
            .db
            .execute_named_cached(sql, named_params! { ":guid": guid })?;

        if changed == 0 {
            log::error!("Failed to create local overlay for GUID {:?}.", guid);
            throw!(ErrorKind::NoSuchRecord(guid.to_owned()));
        }
        Ok(())
    }

    fn mark_mirror_overridden(&self, guid: &str) -> Result<()> {
        self.db.execute_named_cached(
            "UPDATE rec_mirror SET is_overridden = 1 WHERE guid = :guid",
            named_params! { ":guid": guid },
        )?;
        Ok(())
    }

    /// Combines get_vclock with counter_bump, and produces a new VClock with the bumped counter.
    fn get_bumped_vclock(&self, id: &str) -> Result<VClock> {
        let vc = self.get_vclock(id)?;
        let counter = self.counter_bump()?;
        Ok(vc.apply(self.client_id.clone(), counter))
    }

    /// Returns NoSuchRecord if, well, there's no such record.
    fn get_existing_record(&self, rec: &NativeRecord) -> Result<LocalRecord> {
        use crate::{
            schema::desc::{Field, FieldType},
            JsonValue,
        };
        let native = self.info.native_schema();
        let field = native.own_guid();
        assert!(
            matches::matches!(field.ty, FieldType::OwnGuid { .. }),
            "Validation/parsing bug -- field_own_guid must point to an own_guid"
        );
        // Just treat missing and null the same.
        let val = rec.get(&field.local_name).unwrap_or(&JsonValue::Null);
        let guid = Field::validate_guid(&field.local_name, val)?;

        self.get_local_by_id(guid.as_str())?
            .ok_or_else(|| ErrorKind::NoSuchRecord(guid.into()).into())
    }

    pub fn update_record(&self, record: &NativeRecord) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;

        // fails with NoSuchRecord if the record doesn't exist.

        // Potential optimization: we could skip this for schemas that don't use
        // types which need `prev` (untyped_map, record_set, ...)
        let prev = self.get_existing_record(&record)?;

        let (guid, record) = self
            .info
            .native_to_local(record, ToLocalReason::Update { prev })?;

        if self.dupe_exists(&record)? {
            throw!(InvalidRecord::Duplicate);
        }

        // Note: These fail with NoSuchRecord if the record doesn't exist.
        self.ensure_local_overlay_exists(guid.as_str())?;
        self.mark_mirror_overridden(guid.as_str())?;

        let now_ms = MsTime::now();

        let vclock = self.get_bumped_vclock(&guid)?;

        let sql = "
            UPDATE rec_local
            SET local_modified_ms      = :now_millis,
                record_data            = :record,
                vector_clock           = :vclock,
                last_writer_id         = :own_id,
                remerge_schema_version = :schema_ver,
                sync_status            = max(sync_status, :changed)
            WHERE guid = :guid
        ";

        let ct = self.db.execute_named(
            &sql,
            named_params! {
                ":guid": guid,
                ":changed": SyncStatus::Changed as u8,
                ":record": record,
                ":schema_ver": self.info.local.version.to_string(),
                ":now_millis": now_ms,
                ":own_id": self.client_id,
                ":vclock": vclock,
            },
        )?;
        debug_assert_eq!(ct, 1);
        tx.commit()?;
        Ok(())
    }

    pub fn client_id(&self) -> Guid {
        // Guid are essentially free unless the Guid ends up in the "large guid"
        // path, which should never happen for remerge client ids, so it should
        // be fine to always clone this.
        self.client_id.clone()
    }

    pub fn bundle(&self) -> &SchemaBundle {
        &self.info
    }

    fn dupe_exists(&self, record: &LocalRecord) -> Result<bool> {
        let dedupe_field_indexes = &self.info.local.dedupe_on;
        let mut dupe_exists = false;

        // Return false if the schema contains no dedupe_on fields.
        if dedupe_field_indexes.is_empty() {
            return Ok(dupe_exists);
        }

        let db_records = self.get_all().unwrap_or_default();

        // Return false if there are no records in the database.
        if db_records.is_empty() {
            return Ok(dupe_exists);
        }

        dupe_exists = db_records
            .iter()
            .filter(|db_record| {
                let db_id = &db_record.as_obj()["id"];
                let local_id = &record.as_obj()["id"];

                //Filter out updates.
                db_id != local_id
            })
            .any(|db_record| {
                dedupe_field_indexes.iter().all(|dedupe_field_index| {
                    let dedupe_field = &self.info.local.fields[*dedupe_field_index];
                    let db_field_value = &db_record.as_obj()[&dedupe_field.local_name];
                    let local_field_value = &record.as_obj()[&dedupe_field.name];

                    db_field_value == local_field_value
                })
            });

        Ok(dupe_exists)
    }

    /// Have we seen a schema with a required_version above ours? If we have, we
    /// only sync metadata until we get unstuck.
    pub(crate) fn in_sync_lockout(&self) -> Result<bool> {
        let stored = meta::try_get::<String>(self.conn(), meta::SYNC_NATIVE_VERSION_THRESHOLD)?;
        if let Some(v) = stored {
            let ver = match semver::VersionReq::parse(&v) {
                Ok(v) => v,
                Err(e) => {
                    log::error!(
                        "Illegal semver in {:?}: {}",
                        meta::SYNC_NATIVE_VERSION_THRESHOLD.0,
                        e
                    );
                    // Discard it -- it's just to avoid a bunch of expensive and pointless work.
                    meta::delete(self.conn(), meta::SYNC_NATIVE_VERSION_THRESHOLD)?;
                    return Ok(false);
                }
            };
            Ok(!ver.matches(&self.info.native_schema().version))
        } else {
            Ok(false)
        }
    }

    pub fn new_interrupt_handle(&self) -> SqlInterruptHandle {
        SqlInterruptHandle::new(
            self.db.get_interrupt_handle(),
            self.interrupt_counter.clone(),
        )
    }

    /// TODO: this function should return info about additional changes that
    /// need to be made.
    pub(crate) fn upgrade_remote(&mut self, action: &UpgradeRemote) -> Result<()> {
        let target = &self.info().local;
        if action.fresh_server {
            return Ok(());
        }
        let source = if let Some(v) = &action.from {
            v
        } else {
            return Ok(());
        };
        let compare = UpgradeKind::between(source, target);
        if compare == UpgradeKind::RequiresDedupe {
            // How to do this is described in the RFC, just needs impl.
            throw!(ErrorKind::NotYetImplemented(
                "Upgrades that add additional items to dedupe_on".to_string()
            ));
        }
        Ok(())
    }

    pub(crate) fn upgrade_local(&mut self, new_local: Arc<RecordSchema>) -> Result<()> {
        let compare = UpgradeKind::between(&self.info().local, &new_local);
        if compare == UpgradeKind::RequiresDedupe {
            // How to do this is described in the RFC, just needs impl.
            throw!(ErrorKind::NotYetImplemented(
                "Upgrades that add additional items to dedupe_on".to_string()
            ));
        }
        let tx = self.db.unchecked_transaction()?;
        // TODO: Need to make sure `new_local` doesn't reuse the `native` schema's ID.
        let sql = "
            REPLACE INTO remerge_schemas (is_legacy, version, required_version, schema_text)
            VALUES (:legacy, :version, :req_version, :text)
        ";
        let ver_str = new_local.version.to_string();
        self.db.execute_named(
            sql,
            rusqlite::named_params! {
                ":legacy": new_local.legacy,
                ":version": ver_str,
                ":req_version": new_local.required_version.to_string(),
                ":text": &*new_local.source,
            },
        )?;
        meta::put(&self.db, meta::LOCAL_SCHEMA_VERSION, &ver_str)?;
        tx.commit()?;
        self.info.local = new_local;
        Ok(())
    }

    #[inline]
    pub fn begin_interrupt_scope(&self) -> SqlInterruptScope {
        SqlInterruptScope::new(self.interrupt_counter.clone())
    }
}

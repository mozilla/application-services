/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::login::{LocalLogin, Login, MirrorLogin, SyncLoginData, SyncStatus};
use crate::schema;
use crate::sync::{
    self, telemetry, CollectionRequest, IncomingChangeset, OutgoingChangeset, Payload,
    ServerTimestamp, Store,
};
use crate::update_plan::UpdatePlan;
use crate::util;
use lazy_static::lazy_static;
use rusqlite::{
    types::{FromSql, ToSql},
    Connection, NO_PARAMS,
};
use sql_support::{self, ConnExt};
use std::collections::HashSet;
use std::ops::Deref;
use std::path::Path;
use std::result;
use std::time::SystemTime;

pub struct LoginDb {
    pub db: Connection,
}

impl LoginDb {
    pub fn with_connection(db: Connection, encryption_key: Option<&str>) -> Result<Self> {
        #[cfg(test)]
        {
            util::init_test_logging();
        }

        let encryption_pragmas = if let Some(key) = encryption_key {
            // TODO: We probably should support providing a key that doesn't go
            // through PBKDF2 (e.g. pass it in as hex, or use sqlite3_key
            // directly. See https://www.zetetic.net/sqlcipher/sqlcipher-api/#key
            // "Raw Key Data" example. Note that this would be required to open
            // existing iOS sqlcipher databases).
            format!(
                "
                PRAGMA key = '{}';

                -- SQLcipher pre-4.0.0 compatibility. Using SHA1 still
                -- is less than ideal, but should be fine. Real uses of
                -- this (lockbox, etc) use a real random string for the
                -- encryption key, so the reduced KDF iteration count
                -- is fine.
                PRAGMA cipher_page_size = 1024;
                PRAGMA kdf_iter = 64000;
                PRAGMA cipher_hmac_algorithm = HMAC_SHA1;
                PRAGMA cipher_kdf_algorithm = PBKDF2_HMAC_SHA1;
            ",
                sql_support::escape_string_for_pragma(key)
            )
        } else {
            "".to_owned()
        };

        // `temp_store = 2` is required on Android to force the DB to keep temp
        // files in memory, since on Android there's no tmp partition. See
        // https://github.com/mozilla/mentat/issues/505. Ideally we'd only
        // do this on Android, or allow caller to configure it.
        let initial_pragmas = format!(
            "
            {}
            PRAGMA temp_store = 2;
        ",
            encryption_pragmas
        );

        db.execute_batch(&initial_pragmas)?;

        let mut logins = Self { db };
        schema::init(&mut logins)?;
        Ok(logins)
    }

    pub fn open(path: impl AsRef<Path>, encryption_key: Option<&str>) -> Result<Self> {
        Ok(Self::with_connection(
            Connection::open(path)?,
            encryption_key,
        )?)
    }

    pub fn open_in_memory(encryption_key: Option<&str>) -> Result<Self> {
        Ok(Self::with_connection(
            Connection::open_in_memory()?,
            encryption_key,
        )?)
    }
}

impl ConnExt for LoginDb {
    #[inline]
    fn conn(&self) -> &Connection {
        &self.db
    }
}

impl Deref for LoginDb {
    type Target = Connection;
    #[inline]
    fn deref(&self) -> &Connection {
        &self.db
    }
}

// login specific stuff.

impl LoginDb {
    fn mark_as_synchronized(&self, guids: &[&str], ts: ServerTimestamp) -> Result<()> {
        sql_support::each_chunk(guids, |chunk, _| -> Result<()> {
            self.db.execute(
                &format!(
                    "DELETE FROM loginsM WHERE guid IN ({vars})",
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                chunk,
            )?;

            self.db.execute(
                &format!(
                    "
                    INSERT OR IGNORE INTO loginsM (
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

            self.db.execute(
                &format!(
                    "DELETE FROM loginsL WHERE guid IN ({vars})",
                    vars = sql_support::repeat_sql_vars(chunk.len())
                ),
                chunk,
            )?;
            Ok(())
        })?;
        self.set_last_sync(ts)?;
        Ok(())
    }

    // Fetch all the data for the provided IDs.
    // TODO: Might be better taking a fn instead of returning all of it... But that func will likely
    // want to insert stuff while we're doing this so ugh.
    fn fetch_login_data(
        &self,
        records: &[(sync::Payload, ServerTimestamp)],
    ) -> Result<Vec<SyncLoginData>> {
        let mut sync_data = Vec::with_capacity(records.len());
        {
            let mut seen_ids: HashSet<String> = HashSet::with_capacity(records.len());
            for incoming in records.iter() {
                if seen_ids.contains(&incoming.0.id) {
                    throw!(ErrorKind::DuplicateGuid(incoming.0.id.to_string()))
                }
                seen_ids.insert(incoming.0.id.clone());
                sync_data.push(SyncLoginData::from_payload(incoming.0.clone(), incoming.1)?);
            }
        }

        sql_support::each_chunk_mapped(
            &records,
            |r| r.0.id.as_str(),
            |chunk, offset| -> Result<()> {
                // pairs the bound parameter for the guid with an integer index.
                let values_with_idx = sql_support::repeat_display(chunk.len(), ",", |i, f| {
                    write!(f, "({},?)", i + offset)
                });
                let query = format!(
                    "
                WITH to_fetch(guid_idx, fetch_guid) AS (VALUES {vals})
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

                let mut stmt = self.db.prepare(&query)?;

                let rows = stmt.query_and_then(chunk, |row| {
                    let guid_idx_i = row.get_checked::<_, i64>("guid_idx")?;
                    // Hitting this means our math is wrong...
                    assert!(guid_idx_i >= 0);

                    let guid_idx = guid_idx_i as usize;
                    let is_mirror: bool = row.get_checked("is_mirror")?;
                    if is_mirror {
                        sync_data[guid_idx].set_mirror(MirrorLogin::from_row(row)?)?;
                    } else {
                        sync_data[guid_idx].set_local(LocalLogin::from_row(row)?)?;
                    }
                    Ok(())
                })?;
                // `rows` is an Iterator<Item = Result<()>>, so we need to collect to handle the errors.
                rows.collect::<Result<_>>()?;
                Ok(())
            },
        )?;
        Ok(sync_data)
    }

    // It would be nice if this were a batch-ish api (e.g. takes a slice of records and finds dupes
    // for each one if they exist)... I can't think of how to write that query, though.
    fn find_dupe(&self, l: &Login) -> Result<Option<Login>> {
        let form_submit_host_port = l
            .form_submit_url
            .as_ref()
            .and_then(|s| util::url_host_port(&s));
        let args = &[
            (":hostname", &l.hostname as &ToSql),
            (":http_realm", &l.http_realm as &ToSql),
            (":username", &l.username as &ToSql),
            (":form_submit", &form_submit_host_port as &ToSql),
        ];
        let mut query = format!(
            "
            SELECT {common}
            FROM loginsL
            WHERE hostname IS :hostname
              AND httpRealm IS :http_realm
              AND username IS :username",
            common = schema::COMMON_COLS,
        );
        if form_submit_host_port.is_some() {
            // Stolen from iOS
            query += " AND (formSubmitURL = '' OR (instr(formSubmitURL, :form_submit) > 0))";
        } else {
            query += " AND formSubmitURL IS :form_submit"
        }
        Ok(self.try_query_row(&query, args, |row| Login::from_row(row), false)?)
    }

    pub fn get_all(&self) -> Result<Vec<Login>> {
        let mut stmt = self.db.prepare_cached(&GET_ALL_SQL)?;
        let rows = stmt.query_and_then(NO_PARAMS, Login::from_row)?;
        rows.collect::<Result<_>>()
    }

    pub fn get_by_id(&self, id: &str) -> Result<Option<Login>> {
        self.try_query_row(
            &GET_BY_GUID_SQL,
            &[(":guid", &id as &ToSql)],
            Login::from_row,
            true,
        )
    }

    pub fn touch(&self, id: &str) -> Result<()> {
        self.ensure_local_overlay_exists(id)?;
        self.mark_mirror_overridden(id)?;
        let now_ms = util::system_time_ms_i64(SystemTime::now());
        // As on iOS, just using a record doesn't flip it's status to changed.
        // TODO: this might be wrong for lockbox!
        self.execute_named_cached(
            "
            UPDATE loginsL
               SET timeLastUsed = :now_millis,
                   timesUsed = timesUsed + 1,
                   local_modified = :now_millis
               WHERE guid = :guid
                 AND is_deleted = 0",
            &[(":now_millis", &now_ms as &ToSql), (":guid", &id as &ToSql)],
        )?;
        Ok(())
    }

    pub fn add(&self, mut login: Login) -> Result<Login> {
        login.check_valid()?;

        let now_ms = util::system_time_ms_i64(SystemTime::now());

        // Allow an empty GUID to be passed to indicate that we should generate
        // one. (Note that the FFI, does not require that the `id` field be
        // present in the JSON, and replaces it with an empty string if missing).
        if login.id.is_empty() {
            // Our FFI handles panics so this is fine. In practice there's not
            // much we can do here. Using a CSPRNG for this is probably
            // unnecessary, so we likely could fall back to something less
            // fallible eventually, but it's unlikely very much else will work
            // if this fails, so it doesn't matter much.
            login.id = sync::util::random_guid()
                .expect("Failed to generate failed to generate random bytes for GUID");
        }

        // Fill in default metadata.
        // TODO: allow this to be provided for testing?
        login.time_created = now_ms;
        login.time_password_changed = now_ms;
        login.time_last_used = now_ms;
        login.times_used = 1;

        let sql = format!(
            "
            INSERT OR IGNORE INTO loginsL (
                hostname,
                httpRealm,
                formSubmitURL,
                usernameField,
                passwordField,
                timesUsed,
                username,
                password,
                guid,
                timeCreated,
                timeLastUsed,
                timePasswordChanged,
                local_modified,
                is_deleted,
                sync_status
            ) VALUES (
                :hostname,
                :http_realm,
                :form_submit_url,
                :username_field,
                :password_field,
                :times_used,
                :username,
                :password,
                :guid,
                :time_created,
                :time_last_used,
                :time_password_changed,
                :local_modified,
                0, -- is_deleted
                {new} -- sync_status
            )",
            new = SyncStatus::New as u8
        );

        let rows_changed = self.execute_named(
            &sql,
            &[
                (":hostname", &login.hostname as &ToSql),
                (":http_realm", &login.http_realm as &ToSql),
                (":form_submit_url", &login.form_submit_url as &ToSql),
                (":username_field", &login.username_field as &ToSql),
                (":password_field", &login.password_field as &ToSql),
                (":username", &login.username as &ToSql),
                (":password", &login.password as &ToSql),
                (":guid", &login.id as &ToSql),
                (":time_created", &login.time_created as &ToSql),
                (":times_used", &login.times_used as &ToSql),
                (":time_last_used", &login.time_last_used as &ToSql),
                (
                    ":time_password_changed",
                    &login.time_password_changed as &ToSql,
                ),
                (":local_modified", &now_ms as &ToSql),
            ],
        )?;
        if rows_changed == 0 {
            log::error!(
                "Record {:?} already exists (use `update` to update records, not add)",
                login.id
            );
            throw!(ErrorKind::DuplicateGuid(login.id));
        }
        Ok(login)
    }

    pub fn update(&self, login: Login) -> Result<()> {
        login.check_valid()?;
        // Note: These fail with DuplicateGuid if the record doesn't exist.
        self.ensure_local_overlay_exists(login.guid_str())?;
        self.mark_mirror_overridden(login.guid_str())?;

        let now_ms = util::system_time_ms_i64(SystemTime::now());

        let sql = format!(
            "
            UPDATE loginsL
            SET local_modified      = :now_millis,
                timeLastUsed        = :now_millis,
                -- Only update timePasswordChanged if, well, the password changed.
                timePasswordChanged = (CASE
                    WHEN password = :password
                    THEN timePasswordChanged
                    ELSE :now_millis
                END),
                httpRealm           = :http_realm,
                formSubmitURL       = :form_submit_url,
                usernameField       = :username_field,
                passwordField       = :password_field,
                timesUsed           = timesUsed + 1,
                username            = :username,
                password            = :password,
                hostname            = :hostname,
                -- leave New records as they are, otherwise update them to `changed`
                sync_status         = max(sync_status, {changed})
            WHERE guid = :guid",
            changed = SyncStatus::Changed as u8
        );

        self.db.execute_named(
            &sql,
            &[
                (":hostname", &login.hostname as &ToSql),
                (":username", &login.username as &ToSql),
                (":password", &login.password as &ToSql),
                (":http_realm", &login.http_realm as &ToSql),
                (":form_submit_url", &login.form_submit_url as &ToSql),
                (":username_field", &login.username_field as &ToSql),
                (":password_field", &login.password_field as &ToSql),
                (":guid", &login.id as &ToSql),
                (":now_millis", &now_ms as &ToSql),
            ],
        )?;
        Ok(())
    }

    pub fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.db.query_row_named(
            "
            SELECT EXISTS(
                SELECT 1 FROM loginsL
                WHERE guid = :guid AND is_deleted = 0
                UNION ALL
                SELECT 1 FROM loginsM
                WHERE guid = :guid AND is_overridden IS NOT 1
            )",
            &[(":guid", &id as &ToSql)],
            |row| row.get(0),
        )?)
    }

    /// Delete the record with the provided id. Returns true if the record
    /// existed already.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let exists = self.exists(id)?;
        let now_ms = util::system_time_ms_i64(SystemTime::now());

        // Directly delete IDs that have not yet been synced to the server
        self.execute_named(
            &format!(
                "
            DELETE FROM loginsL
            WHERE guid = :guid
              AND sync_status = {status_new}",
                status_new = SyncStatus::New as u8
            ),
            &[(":guid", &id as &ToSql)],
        )?;

        // For IDs that have, mark is_deleted and clear sensitive fields
        self.execute_named(
            &format!(
                "
            UPDATE loginsL
            SET local_modified = :now_ms,
                sync_status = {status_changed},
                is_deleted = 1,
                password = '',
                hostname = '',
                username = ''
            WHERE guid = :guid",
                status_changed = SyncStatus::Changed as u8
            ),
            &[(":now_ms", &now_ms as &ToSql), (":guid", &id as &ToSql)],
        )?;

        // Mark the mirror as overridden
        self.execute_named(
            "UPDATE loginsM SET is_overridden = 1 WHERE guid = :guid",
            &[(":guid", &id as &ToSql)],
        )?;

        // If we don't have a local record for this ID, but do have it in the mirror
        // insert a tombstone.
        self.execute_named(&format!("
            INSERT OR IGNORE INTO loginsL
                    (guid, local_modified, is_deleted, sync_status, hostname, timeCreated, timePasswordChanged, password, username)
            SELECT   guid, :now_ms,        1,          {changed},   '',       timeCreated, :now_ms,                   '',       ''
            FROM loginsM
            WHERE guid = :guid",
            changed = SyncStatus::Changed as u8),
            &[(":now_ms", &now_ms as &ToSql),
              (":guid", &id as &ToSql)])?;

        Ok(exists)
    }

    fn mark_mirror_overridden(&self, guid: &str) -> Result<()> {
        self.execute_named_cached(
            "
            UPDATE loginsM SET
            is_overridden = 1
            WHERE guid = :guid
        ",
            &[(":guid", &guid as &ToSql)],
        )?;
        Ok(())
    }

    fn ensure_local_overlay_exists(&self, guid: &str) -> Result<()> {
        let already_have_local: bool = self.db.query_row_named(
            "SELECT EXISTS(SELECT 1 FROM loginsL WHERE guid = :guid)",
            &[(":guid", &guid as &ToSql)],
            |row| row.get(0),
        )?;

        if already_have_local {
            return Ok(());
        }

        log::debug!("No overlay; cloning one for {:?}.", guid);
        let changed = self.clone_mirror_to_overlay(guid)?;
        if changed == 0 {
            log::error!("Failed to create local overlay for GUID {:?}.", guid);
            throw!(ErrorKind::NoSuchRecord(guid.to_owned()));
        }
        Ok(())
    }

    fn clone_mirror_to_overlay(&self, guid: &str) -> Result<usize> {
        Ok(self.execute_named_cached(&*CLONE_SINGLE_MIRROR_SQL, &[(":guid", &guid as &ToSql)])?)
    }

    pub fn reset(&self) -> Result<()> {
        log::info!("Executing reset on password store!");
        self.execute_all(&[
            &*CLONE_ENTIRE_MIRROR_SQL,
            "DELETE FROM loginsM",
            &format!("UPDATE loginsL SET sync_status = {}", SyncStatus::New as u8),
        ])?;
        self.set_last_sync(ServerTimestamp(0.0))?;
        // TODO: Should we clear global_state?
        Ok(())
    }

    pub fn wipe(&self) -> Result<()> {
        log::info!("Executing wipe on password store!");
        let now_ms = util::system_time_ms_i64(SystemTime::now());
        self.execute(
            &format!(
                "DELETE FROM loginsL WHERE sync_status = {new}",
                new = SyncStatus::New as u8
            ),
            NO_PARAMS,
        )?;
        self.execute_named(
            &format!(
                "
                UPDATE loginsL
                SET local_modified = :now_ms,
                    sync_status = {changed},
                    is_deleted = 1,
                    password = '',
                    hostname = '',
                    username = ''
                WHERE is_deleted = 0",
                changed = SyncStatus::Changed as u8
            ),
            &[(":now_ms", &now_ms as &ToSql)],
        )?;

        self.execute("UPDATE loginsM SET is_overridden = 1", NO_PARAMS)?;

        self.execute_named(
            &format!("
                INSERT OR IGNORE INTO loginsL
                      (guid, local_modified, is_deleted, sync_status, hostname, timeCreated, timePasswordChanged, password, username)
                SELECT guid, :now_ms,        1,          {changed},   '',       timeCreated, :now_ms,             '',       ''
                FROM loginsM",
                changed = SyncStatus::Changed as u8),
            &[(":now_ms", &now_ms as &ToSql)])?;

        Ok(())
    }

    pub fn wipe_local(&self) -> Result<()> {
        log::info!("Executing wipe_local on password store!");
        self.execute_all(&[
            "DELETE FROM loginsL",
            "DELETE FROM loginsM",
            "DELETE FROM loginsSyncMeta",
        ])?;
        Ok(())
    }

    fn reconcile(
        &self,
        records: Vec<SyncLoginData>,
        server_now: ServerTimestamp,
        telem: &mut telemetry::EngineIncoming,
    ) -> Result<UpdatePlan> {
        let mut plan = UpdatePlan::default();

        for mut record in records {
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
                    if let Some(dupe) = self.find_dupe(&upstream)? {
                        log::debug!(
                            "  Incoming record {} was is a dupe of local record {}",
                            upstream.id,
                            dupe.id
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

    fn execute_plan(&self, plan: UpdatePlan) -> Result<()> {
        // Because rusqlite want a mutable reference to create a transaction
        // (as a way to save us from ourselves), we side-step that by creating
        // it manually.
        let tx = self.db.unchecked_transaction()?;
        plan.execute(&tx)?;
        tx.commit()?;
        Ok(())
    }

    pub fn fetch_outgoing(&self, st: ServerTimestamp) -> Result<OutgoingChangeset> {
        let mut outgoing = OutgoingChangeset::new("passwords".into(), st);
        let mut stmt = self.db.prepare_cached(&format!(
            "
            SELECT * FROM loginsL
            WHERE sync_status IS NOT {synced}",
            synced = SyncStatus::Synced as u8
        ))?;
        let rows = stmt.query_and_then(NO_PARAMS, |row| {
            Ok(if row.get_checked::<_, bool>("is_deleted")? {
                Payload::new_tombstone(row.get_checked::<_, String>("guid")?)
            } else {
                let login = Login::from_row(row)?;
                Payload::from_record(login)?
            })
        })?;
        outgoing.changes = rows.collect::<Result<_>>()?;

        Ok(outgoing)
    }

    fn do_apply_incoming(
        &self,
        inbound: IncomingChangeset,
        telem: &mut telemetry::EngineIncoming,
    ) -> Result<OutgoingChangeset> {
        let data = self.fetch_login_data(&inbound.changes)?;
        let plan = self.reconcile(data, inbound.timestamp, telem)?;
        self.execute_plan(plan)?;
        Ok(self.fetch_outgoing(inbound.timestamp)?)
    }

    fn put_meta(&self, key: &str, value: &ToSql) -> Result<()> {
        self.execute_named_cached(
            "REPLACE INTO loginsSyncMeta (key, value) VALUES (:key, :value)",
            &[(":key", &key as &ToSql), (":value", value)],
        )?;
        Ok(())
    }

    fn get_meta<T: FromSql>(&self, key: &str) -> Result<Option<T>> {
        Ok(self.try_query_row(
            "SELECT value FROM loginsSyncMeta WHERE key = :key",
            &[(":key", &key as &ToSql)],
            |row| Ok::<_, Error>(row.get_checked(0)?),
            true,
        )?)
    }

    fn set_last_sync(&self, last_sync: ServerTimestamp) -> Result<()> {
        log::debug!("Updating last sync to {}", last_sync);
        let last_sync_millis = last_sync.as_millis() as i64;
        self.put_meta(schema::LAST_SYNC_META_KEY, &last_sync_millis)
    }

    fn get_last_sync(&self) -> Result<Option<ServerTimestamp>> {
        Ok(self
            .get_meta::<i64>(schema::LAST_SYNC_META_KEY)?
            .map(|millis| ServerTimestamp(millis as f64 / 1000.0)))
    }

    pub fn set_global_state(&self, global_state: Option<String>) -> Result<()> {
        let to_write = match global_state {
            Some(ref s) => s,
            None => "",
        };
        self.put_meta(schema::GLOBAL_STATE_META_KEY, &to_write)
    }

    pub fn get_global_state(&self) -> Result<Option<String>> {
        self.get_meta::<String>(schema::GLOBAL_STATE_META_KEY)
    }
}

impl Store for LoginDb {
    fn collection_name(&self) -> &'static str {
        "passwords"
    }

    fn apply_incoming(
        &self,
        inbound: IncomingChangeset,
        telem: &mut telemetry::EngineIncoming,
    ) -> result::Result<OutgoingChangeset, failure::Error> {
        Ok(self.do_apply_incoming(inbound, telem)?)
    }

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: &[String],
    ) -> result::Result<(), failure::Error> {
        Ok(self.mark_as_synchronized(
            &records_synced
                .iter()
                .map(|r| r.as_str())
                .collect::<Vec<_>>(),
            new_timestamp,
        )?)
    }

    fn get_collection_request(&self) -> result::Result<CollectionRequest, failure::Error> {
        let since = self.get_last_sync()?.unwrap_or_default();
        Ok(CollectionRequest::new("passwords").full().newer_than(since))
    }

    fn reset(&self) -> result::Result<(), failure::Error> {
        LoginDb::reset(self)?;
        Ok(())
    }

    fn wipe(&self) -> result::Result<(), failure::Error> {
        LoginDb::wipe(self)?;
        Ok(())
    }
}

lazy_static! {
    static ref GET_ALL_SQL: String = format!(
        "
        SELECT {common_cols} FROM loginsL WHERE is_deleted = 0
        UNION ALL
        SELECT {common_cols} FROM loginsM WHERE is_overridden = 0
    ",
        common_cols = schema::COMMON_COLS,
    );
    static ref GET_BY_GUID_SQL: String = format!(
        "
        SELECT {common_cols}
        FROM loginsL
        WHERE is_deleted = 0
          AND guid = :guid

        UNION ALL

        SELECT {common_cols}
        FROM loginsM
        WHERE is_overridden IS NOT 1
          AND guid = :guid
        ORDER BY hostname ASC

        LIMIT 1
    ",
        common_cols = schema::COMMON_COLS,
    );
    static ref CLONE_ENTIRE_MIRROR_SQL: String = format!(
        "
        INSERT OR IGNORE INTO loginsL ({common_cols}, local_modified, is_deleted, sync_status)
        SELECT {common_cols}, NULL AS local_modified, 0 AS is_deleted, 0 AS sync_status
        FROM loginsM",
        common_cols = schema::COMMON_COLS,
    );
    static ref CLONE_SINGLE_MIRROR_SQL: String =
        format!("{} WHERE guid = :guid", &*CLONE_ENTIRE_MIRROR_SQL,);
}

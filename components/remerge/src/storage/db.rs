/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::SyncStatus;
use crate::error::*;
use crate::ms_time::MsTime;
use crate::vclock::{Counter, VClock};
use rusqlite::Connection;
use serde_json::Value as JsonValue;
use sql_support::ConnExt;
use std::sync::Mutex;
use sync_guid::Guid;

pub struct RemergeDb {
    db: Connection,
    info: super::bootstrap::RemergeInfo,
}

lazy_static::lazy_static! {
    static ref DB_INIT_MUTEX: Mutex<()> = Mutex::new(());
}

impl RemergeDb {
    pub fn with_connection(
        mut db: Connection,
        native: super::NativeSchemaInfo<'_>,
    ) -> Result<Self> {
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
        let info = super::bootstrap::load_or_bootstrap(&tx, native)?;
        tx.commit()?;
        Ok(RemergeDb { db, info })
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
            rusqlite::named_params! { ":guid": id },
            |row| row.get(0),
        )?)
    }

    pub fn create(&self, record_info: JsonValue) -> Result<sync_guid::Guid> {
        let mut id = Guid::random();
        let mut to_insert = serde_json::Map::default();
        let record_obj = record_info
            .as_object()
            .ok_or_else(|| InvalidRecord::NotJsonObject)?;
        for field in &self.info.local.fields {
            let native_field = &self
                .info
                .native
                .fields
                .iter()
                .find(|f| f.name == field.name);
            let local_name = native_field
                .map(|n| n.local_name.as_str())
                .unwrap_or_else(|| field.name.as_str());
            let is_guid = crate::schema::FieldKind::OwnGuid == field.ty.kind();
            if let Some(v) = record_obj.get(local_name) {
                let fixed = field.validate(v.clone())?;
                if is_guid {
                    if let JsonValue::String(s) = &fixed {
                        id = Guid::from(s.as_str());
                    } else {
                        unreachable!("Field::validate checks this");
                    }
                }
                to_insert.insert(field.name.clone(), fixed);
            } else if let Some(def) = field.ty.get_default() {
                to_insert.insert(field.name.clone(), def);
            } else if is_guid {
                to_insert.insert(field.name.clone(), id.to_string().into());
            } else if field.required {
                throw!(InvalidRecord::MissingRequiredField(local_name.to_owned()));
            }
        }
        let tx = self.db.unchecked_transaction()?;
        // TODO: Search DB for dupes based on the value of the fields listed in dedupe_on.
        let id_exists = self.exists(id.as_ref())?;
        if id_exists {
            throw!(InvalidRecord::IdNotUnique);
        }
        let ctr = super::meta::get::<i64>(&self.db, super::meta::CHANGE_COUNTER)? + 1;
        super::meta::put(&self.db, super::meta::CHANGE_COUNTER, &ctr)?;
        let vclock = VClock::new(self.info.client_id.clone(), ctr as Counter);
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
            rusqlite::named_params! {
                ":guid": id,
                ":schema_ver": self.info.local.version.to_string(),
                ":now": now,
                ":status": SyncStatus::New as u8,
                ":vclock": vclock,
                ":client_id": self.info.client_id,
            },
        )?;
        tx.commit()?;
        Ok(id)
    }
}

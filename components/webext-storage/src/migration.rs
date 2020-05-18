/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use rusqlite::{Connection, OpenFlags, Transaction, NO_PARAMS};
use serde_json::{Map, Value};
use sql_support::ConnExt;
use std::path::PathBuf;

// Simple migration from the "old" kinto-with-sqlite-backing implementation
// to ours.
// Could almost be trivially done in JS using the regular public API if not
// for:
// * We don't want to enforce the same quotas when migrating.
// * We'd rather do the entire migration in a single transaction for perf
//   reasons.

// The sqlite database we migrate from has a very simple structure:
// * table collection_data with columns collection_name, record_id and record
// * `collection_name` is a string of form "default/{extension_id}"
// * `record_id` is `key-{key}`
// * `record` is a string with json, of form: {
//     id: {the record id repeated},
//     key: {the key},
//     data: {the actual data},
//     _status: {sync status},
//     last_modified: {timestamp},
//  }
// So the info we need is stored somewhat redundantly.
// Further:
// * There's a special collection_name "default/storage-sync-crypto" that
//   we don't want to migrate. Its record_id is 'keys' and its json has no
//   `data`

// We don't enforce a real quota, but don't want to exceed the
// sync server's BSO size and also just be generally sane - so stop at 1.5MB
// and also 2k of keys
const MAX_LEN: usize = 1_512_000;
const MAX_KEYS: usize = 2048;

// The struct we read from the DB.
struct LegacyRow {
    col_name: String, // collection_name column
    record: String,   // record column
}

impl LegacyRow {
    // Parse the 2 columns from the DB into the data we need to insert into
    // our target database.
    fn parse(&self) -> Option<Parsed<'_>> {
        if self.col_name.len() < 8 {
            log::trace!("collection_name of '{}' is too short", self.col_name);
            return None;
        }
        if &self.col_name[..8] != "default/" {
            log::trace!("collection_name of '{}' isn't ours", self.col_name);
            return None;
        }
        let ext_id = &self.col_name[8..];
        let mut record_map = match serde_json::from_str(&self.record) {
            Ok(Value::Object(m)) => m,
            Ok(o) => {
                log::info!("skipping non-json-object 'record' column");
                log::trace!("record value is json, but not an object: {}", o);
                return None;
            }
            Err(e) => {
                log::info!("skipping non-json 'record' column");
                log::trace!("record value isn't json: {}", e);
                return None;
            }
        };

        let key = match record_map.remove("key") {
            Some(Value::String(s)) if !s.is_empty() => s,
            Some(o) => {
                log::trace!("key is json but not a string: {}", o);
                return None;
            }
            _ => {
                log::trace!("key doesn't exist in the map");
                return None;
            }
        };
        let data = match record_map.remove("data") {
            Some(d) => d,
            _ => {
                log::trace!("data doesn't exist in the map");
                return None;
            }
        };
        Some(Parsed { ext_id, key, data })
    }
}

// The info we parse from the raw DB strings.
struct Parsed<'a> {
    ext_id: &'a str,
    key: String,
    data: serde_json::Value,
}

pub fn migrate(tx: &Transaction<'_>, filename: &PathBuf) -> Result<usize> {
    // We do the grouping manually, collecting string values as we go.
    let mut last_ext_id = "".to_string();
    let mut curr_values: Vec<(String, serde_json::Value)> = Vec::new();
    let mut curr_value_len = 0;
    let mut num_extensions = 0;
    for row in read_rows(filename)? {
        log::trace!("processing '{}' - '{}'", row.col_name, row.record);
        let parsed = match row.parse() {
            Some(p) => p,
            None => continue,
        };
        // Do our "grouping"
        if parsed.ext_id != last_ext_id {
            if last_ext_id != "" && !curr_values.is_empty() {
                // a different extension id - write what we have to the DB.
                if do_insert(tx, &last_ext_id, curr_values) {
                    num_extensions += 1;
                }
            }
            last_ext_id = parsed.ext_id.to_string();
            curr_values = Vec::new();
            curr_value_len = 0;
        }
        // no 'else' here - must also enter this block on ext_id change.
        if parsed.ext_id == last_ext_id {
            // Do our "quota but not a quota" thing.
            let new_len = curr_value_len + parsed.data.to_string().len() + parsed.key.len();
            if new_len < MAX_LEN && curr_values.len() < MAX_KEYS {
                curr_values.push((parsed.key.to_string(), parsed.data));
                curr_value_len = new_len;
            }
            log::trace!(
                "extension {} now has {} bytes, {} keys",
                parsed.ext_id,
                curr_value_len,
                curr_values.len()
            );
        }
    }
    // and the last one
    if last_ext_id != "" && !curr_values.is_empty() {
        // a different extension id - write what we have to the DB.
        if do_insert(tx, &last_ext_id, curr_values) {
            num_extensions += 1;
        }
    }
    log::info!("migrated {} extensions", num_extensions);
    Ok(num_extensions)
}

fn read_rows(filename: &PathBuf) -> Result<Vec<LegacyRow>> {
    let flags = OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_READ_ONLY;
    let src_conn = Connection::open_with_flags(&filename, flags)?;
    let mut stmt = src_conn.prepare(
        "SELECT collection_name, record FROM collection_data
         ORDER BY collection_name",
    )?;
    let rows = stmt
        .query_and_then(NO_PARAMS, |row| -> Result<LegacyRow> {
            Ok(LegacyRow {
                col_name: row.get(0)?,
                record: row.get(1)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

fn do_insert(tx: &Transaction<'_>, ext_id: &str, vals: Vec<(String, Value)>) -> bool {
    let mut map = Map::with_capacity(vals.len());
    for (key, val) in vals {
        map.insert(key, val);
    }
    match tx.execute_named_cached(
        "INSERT OR REPLACE INTO storage_sync_data(ext_id, data, sync_change_counter)
         VALUES (:ext_id, :data, 1)",
        rusqlite::named_params! {
            ":ext_id": &ext_id,
            ":data": &Value::Object(map),
        },
    ) {
        Ok(_) => {
            log::trace!("successfully wrote data for {}", ext_id);
            true
        }
        Err(e) => {
            log::error!("failed to write data: {}", e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api;
    use crate::db::{test::new_mem_db, StorageDb};
    use serde_json::json;
    use tempfile::tempdir;

    // Create a test database, populate it via the callback, migrate it, and
    // return a connection to the new, migrated DB for further checking.
    fn do_migrate<F>(num_expected: usize, f: F) -> StorageDb
    where
        F: FnOnce(&Connection),
    {
        let tmpdir = tempdir().unwrap();
        let path = tmpdir.path().join("source.db");
        let flags = OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_READ_WRITE;
        let mut conn = Connection::open_with_flags(path, flags).expect("open should work");
        let tx = conn.transaction().expect("should be able to get a tx");
        tx.execute_batch(
            "CREATE TABLE collection_data (
                collection_name TEXT,
                record_id TEXT,
                record TEXT
            );",
        )
        .expect("create should work");
        f(&tx);
        tx.commit().expect("should commit");
        conn.close().expect("close should work");

        // now migrate
        let mut db = new_mem_db();
        let tx = db.transaction().expect("tx should work");

        let num = migrate(&tx, &tmpdir.path().join("source.db")).expect("migrate should work");
        tx.commit().expect("should work");
        assert_eq!(num, num_expected);
        db
    }

    fn assert_has(c: &Connection, ext_id: &str, expect: Value) {
        assert_eq!(
            api::get(c, ext_id, json!(null)).expect("should get"),
            expect
        );
    }

    #[allow(clippy::unreadable_literal)]
    #[test]
    fn test_happy_paths() {
        // some real data.
        let conn = do_migrate(2, |c| {
            c.execute_batch(
                r#"INSERT INTO collection_data(collection_name, record)
                    VALUES
                    ('default/{e7fefcf3-b39c-4f17-5215-ebfe120a7031}', '{"id":"key-userWelcomed","key":"userWelcomed","data":1570659224457,"_status":"synced","last_modified":1579755940527}'),
                    ('default/{e7fefcf3-b39c-4f17-5215-ebfe120a7031}', '{"id":"key-isWho","key":"isWho","data":"4ec8109f","_status":"synced","last_modified":1579755940497}'),
                    ('default/storage-sync-crypto', '{"id":"keys","keys":{"default":["rQ=","lR="],"collections":{"extension@redux.devtools":["Bd=","ju="]}}}'),
                    ('default/https-everywhere@eff.org', '{"id":"key-userRules","key":"userRules","data":[],"_status":"synced","last_modified":1570079920045}'),
                    ('default/https-everywhere@eff.org', '{"id":"key-ruleActiveStates","key":"ruleActiveStates","data":{},"_status":"synced","last_modified":1570079919993}'),
                    ('default/https-everywhere@eff.org', '{"id":"key-migration_5F_version","key":"migration_version","data":2,"_status":"synced","last_modified":1570079919966}')
                    "#,
            ).expect("should popuplate")
        });

        assert_has(
            &conn,
            "{e7fefcf3-b39c-4f17-5215-ebfe120a7031}",
            json!({"userWelcomed": 1570659224457u64, "isWho": "4ec8109f"}),
        );
        assert_has(
            &conn,
            "https-everywhere@eff.org",
            json!({"userRules": [], "ruleActiveStates": {}, "migration_version": 2}),
        );
    }

    #[test]
    fn test_sad_paths() {
        do_migrate(0, |c| {
            c.execute_batch(
                r#"INSERT INTO collection_data(collection_name, record)
                    VALUES
                    ('default/test', '{"key":2,"data":1}'), -- key not a string
                    ('default/test', '{"key":"","data":1}'), -- key empty string
                    ('default/test', '{"xey":"k","data":1}'), -- key missing
                    ('default/test', '{"key":"k","xata":1}'), -- data missing
                    ('default/test', '{"key":"k","data":1'), -- invalid json
                    ('xx/test', '{"key":"k","data":1}'), -- bad key format
                    ('default', '{"key":"k","data":1}'), -- bad key format 2
                    ('default/', '{"key":"k","data":1}'), -- bad key format 3
                    ('defaultx/test', '{"key":"k","data":1}'), -- bad key format 4
                    ('', '') -- empty strings
                    "#,
            )
            .expect("should populate");
        });
    }

    #[test]
    fn test_too_long() {
        let conn = do_migrate(1, |c| {
            for i in 0..1000 {
                c.execute_batch(&format!(
                    r#"INSERT INTO collection_data(collection_name, record)
                           VALUES ('default/too_long', '{{"key":"{}","data":"{}"}}')"#,
                    i,
                    "x".repeat(2000)
                ))
                .expect("should populate");
            }
        });
        let data = api::get(&conn, "too_long", json!(null)).expect("get should work");
        // The way we calculate bytes is designed to be "fast and close enough"
        // rather than the exact number of bytes in the final json repr - so we
        // might end up with a few bytes over our limit.
        // So just check our keys are far less then we expect.
        let nkeys = data.as_object().unwrap().len();
        assert!(nkeys > 0);
        assert!(nkeys < MAX_KEYS);
    }

    #[test]
    fn test_too_many_keys() {
        let conn = do_migrate(1, |c| {
            for i in 0..2050 {
                c.execute_batch(&format!(
                    r#"INSERT INTO collection_data(collection_name, record)
                           VALUES ('default/too_many', '{{"key":"{}","data":"yo"}}')"#,
                    i
                ))
                .expect("should populate");
            }
        });
        let data = api::get(&conn, "too_many", json!(null)).expect("get should work");
        assert_eq!(data.as_object().unwrap().len(), MAX_KEYS);
    }
}

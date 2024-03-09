/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use crate::error::warn;
use rusqlite::Transaction;
use sql_support::open_database;

const CREATE_TABLE_PUSH_SQL: &str = include_str!("schema.sql");

pub struct PushConnectionInitializer;

impl open_database::ConnectionInitializer for PushConnectionInitializer {
    const NAME: &'static str = "push db";
    const END_VERSION: u32 = 3;

    // This is such a simple database that we do almost nothing!
    // * We have no foreign keys, so `PRAGMA foreign_keys = ON;` is pointless.
    // * We have no temp tables, so `PRAGMA temp_store = 2;` is pointless.
    // * We don't even use transactions, so `PRAGMA journal_mode=WAL;` is pointless.
    // * We have a tiny number of different SQL statements, so
    //   set_prepared_statement_cache_capacity is pointless.
    // * We have no SQL functions.
    // Kinda makes you wonder why we use a sql db at all :)
    // So - no "prepare" and no "finish" methods.
    fn init(&self, db: &Transaction<'_>) -> open_database::Result<()> {
        db.execute_batch(CREATE_TABLE_PUSH_SQL)?;
        Ok(())
    }

    fn upgrade_from(&self, db: &Transaction<'_>, version: u32) -> open_database::Result<()> {
        match version {
            0 => db.execute_batch(CREATE_TABLE_PUSH_SQL)?,
            1 => db.execute_batch(CREATE_TABLE_PUSH_SQL)?,
            2 => {
                // We dropped the `uaid` and `native_id` columns and added a constraint that scope
                // must not be an empty string and must be unique.
                let sql = format!(
                    "
                    -- rename the old table.
                    ALTER TABLE push_record RENAME TO push_record_old;
                    -- create the new table with the new schema.
                    {CREATE_TABLE_PUSH_SQL};
                    -- move the data across.
                    INSERT OR IGNORE INTO push_record ({COMMON_COLS})
                    SELECT {COMMON_COLS} FROM push_record_old WHERE length(scope) > 0;
                    -- drop the old table
                    DROP TABLE push_record_old;",
                    CREATE_TABLE_PUSH_SQL = CREATE_TABLE_PUSH_SQL,
                    COMMON_COLS = COMMON_COLS,
                );
                db.execute_batch(&sql)?;
            }
            other => {
                warn!(
                    "Loaded future schema version {} (we only understand version {}). \
                    Optimistically ",
                    other,
                    Self::END_VERSION
                )
            }
        };
        Ok(())
    }
}

pub const COMMON_COLS: &str = "
    channel_id,
    endpoint,
    scope,
    key,
    ctime,
    app_server_key
";

#[cfg(test)]
mod test {
    use crate::internal::storage::db::{PushDb, Storage};
    use rusqlite::{Connection, OpenFlags};
    use sql_support::ConnExt;

    const CREATE_V2_SCHEMA: &str = include_str!("test/schema_v2.sql");

    #[test]
    fn test_migrate_v2_v3() {
        error_support::init_for_tests();
        let tempdir = tempfile::tempdir().unwrap();
        let path = tempdir.path().join("push_v2.sql");

        let conn = Connection::open_with_flags(path.clone(), OpenFlags::default()).unwrap();
        conn.execute_batch(CREATE_V2_SCHEMA).unwrap();

        // insert some stuff
        conn.execute_batch(
            r#"
            INSERT INTO push_record (
                uaid,    channel_id, endpoint, scope,  key,     ctime, app_server_key, native_id
            ) VALUES
                ("id-1", "cid1",     "ep-1",   "sc-1", x'1234', 1,    "ask-1",         "nid-1"),
                -- duplicate scope, which isn't allowed in the new schema
                ("id-2", "cid2",     "ep-2",   "sc-1", x'5678', 2,    "ask-2",         "nid-2"),
                -- empty scope, which isn't allowed in the new schema
                ("id-3", "cid3",     "ep-3",   "",     x'0000', 3,    "ask-3",         "nid-3")
            ;
            INSERT into meta_data (
                key, value
            ) VALUES
                ("key-1", "value-1"),
                ("key-2", "value-2")
            "#,
        )
        .unwrap();

        // reopen the database.
        drop(conn);
        let db = PushDb::open(path).expect("should open");

        // Should only have 1 row in push_record
        assert_eq!(
            db.query_one::<u32>("SELECT COUNT(*) FROM push_record")
                .unwrap(),
            1
        );
        let record = db
            .get_record("cid1")
            .expect("should work")
            .expect("should get a record");
        assert_eq!(record.channel_id, "cid1");
        assert_eq!(record.endpoint, "ep-1");
        assert_eq!(record.scope, "sc-1");
        assert_eq!(record.key, [0x12, 0x34]);
        assert_eq!(record.ctime.0, 1);
        assert_eq!(record.app_server_key.unwrap(), "ask-1");

        // But both metadata ones.
        assert_eq!(
            db.db
                .query_one::<u32>("SELECT COUNT(*) FROM meta_data")
                .unwrap(),
            2
        );
        assert_eq!(db.get_meta("key-1").unwrap().unwrap(), "value-1");
        assert_eq!(db.get_meta("key-2").unwrap().unwrap(), "value-2");
    }
}

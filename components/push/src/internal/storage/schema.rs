/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use rusqlite::Transaction;
use sql_support::open_database;

const CREATE_TABLE_PUSH_SQL: &str = include_str!("schema.sql");

pub struct PushConnectionInitializer;

impl open_database::ConnectionInitializer for PushConnectionInitializer {
    const NAME: &'static str = "push db";
    const END_VERSION: u32 = 2;

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
            other => {
                log::warn!(
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
    uaid,
    channel_id,
    endpoint,
    scope,
    key,
    ctime,
    app_server_key,
    native_id
";

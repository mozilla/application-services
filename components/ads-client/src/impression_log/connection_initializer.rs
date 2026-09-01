/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::{vtab::array, Connection};
use sql_support::open_database;
use std::time::Duration;

pub struct ImpressionLogConnectionInitializer {}

impl open_database::ConnectionInitializer for ImpressionLogConnectionInitializer {
    const NAME: &'static str = "impression_log";
    const END_VERSION: u32 = 1;

    fn prepare(&self, conn: &Connection, _db_empty: bool) -> open_database::Result<()> {
        conn.execute_batch("PRAGMA journal_mode=wal;")?;
        array::load_module(conn)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        Ok(())
    }

    fn init(&self, tx: &rusqlite::Transaction<'_>) -> open_database::Result<()> {
        const SCHEMA: &str = "
            CREATE TABLE IF NOT EXISTS impression_log (
                cap_key TEXT NOT NULL,
                recorded_at INTEGER NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_impression_log_pk ON impression_log(cap_key, recorded_at);
        ";
        // If the schema fails to initialize, it might be corrupted or outdated so we drop the table and try again
        if tx.execute_batch(SCHEMA).is_err() {
            tx.execute_batch("DROP TABLE IF EXISTS impression_log")?;
            tx.execute_batch(SCHEMA)?;
        }
        Ok(())
    }

    fn upgrade_from(
        &self,
        conn: &rusqlite::Transaction<'_>,
        version: u32,
    ) -> open_database::Result<()> {
        match version {
            0 => self.init(conn),
            _ => Err(open_database::Error::IncompatibleVersion(version)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use sql_support::open_database::ConnectionInitializer;

    #[test]
    fn test_corrupted_schema_is_recreated() {
        let mut conn = Connection::open_in_memory().unwrap();
        let initializer = ImpressionLogConnectionInitializer {};

        // Create a corrupted table missing needed index columns
        conn.execute_batch("CREATE TABLE impression_log (cap_key TEXT);")
            .unwrap();

        // Run init - should drop the corrupted table and recreate it properly
        let tx = conn.transaction().unwrap();
        initializer.init(&tx).unwrap();
        tx.commit().unwrap();

        // Verify the table was recreated with correct schema by checking column count
        let column_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('impression_log')",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(
            column_count > 1,
            "Table should have more than 1 column after recreation"
        );
    }
}

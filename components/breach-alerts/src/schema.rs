/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::debug;
use rusqlite::{Connection, Transaction};
use sql_support::open_database::{
    ConnectionInitializer as MigrationLogic, Error as MigrationError, Result as MigrationResult,
};

const CREATE_SCHEMA_SQL: &str = include_str!("../sql/create_schema.sql");

pub struct BreachAlertsConnectionInitializer;

impl MigrationLogic for BreachAlertsConnectionInitializer {
    const NAME: &'static str = "breach alerts db";
    const END_VERSION: u32 = 1;

    fn prepare(&self, conn: &Connection, _db_empty: bool) -> MigrationResult<()> {
        let initial_pragmas = "
            -- We don't care about temp tables being persisted to disk.
            PRAGMA temp_store = 2;
            -- we unconditionally want write-ahead-logging mode
            PRAGMA journal_mode=WAL;
            -- foreign keys seem worth enforcing!
            PRAGMA foreign_keys = ON;
        ";
        conn.execute_batch(initial_pragmas)?;
        conn.set_prepared_statement_cache_capacity(128);
        Ok(())
    }

    fn init(&self, db: &Transaction<'_>) -> MigrationResult<()> {
        debug!("Creating schema");
        db.execute_batch(CREATE_SCHEMA_SQL)?;
        Ok(())
    }

    fn upgrade_from(&self, _db: &Transaction<'_>, version: u32) -> MigrationResult<()> {
        Err(MigrationError::IncompatibleVersion(version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test::new_mem_db;

    #[test]
    fn test_create_schema_twice() {
        let db = new_mem_db();
        let conn = db.get_connection().expect("should retrieve connection");
        conn.execute_batch(CREATE_SCHEMA_SQL)
            .expect("should allow running twice");
    }
}

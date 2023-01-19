/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Tabs is a bit special - it's a trivial SQL schema and is only used as a persistent
// cache, and the semantics of the "tabs" collection means there's no need for
// syncChangeCounter/syncStatus nor a mirror etc.

use rusqlite::{Connection, Transaction};
use sql_support::{
    open_database::{
        ConnectionInitializer as MigrationLogic, Error as MigrationError, Result as MigrationResult,
    },
    ConnExt,
};

// The record is the TabsRecord struct in json and this module doesn't need to deserialize, so we just
// store each client as its own row.
const CREATE_SCHEMA_SQL: &str = "
    CREATE TABLE IF NOT EXISTS tabs (
        guid            TEXT NOT NULL PRIMARY KEY,
        record          TEXT NOT NULL,
        last_modified   INTEGER NOT NULL
    );
";

const CREATE_META_TABLE_SQL: &str = "
    CREATE TABLE IF NOT EXISTS moz_meta (
        key    TEXT PRIMARY KEY,
        value  NOT NULL
    )
";

pub(crate) static LAST_SYNC_META_KEY: &str = "last_sync_time";
pub(crate) static GLOBAL_SYNCID_META_KEY: &str = "global_sync_id";
pub(crate) static COLLECTION_SYNCID_META_KEY: &str = "tabs_sync_id";
// Tabs stores this in the meta table due to a unique requirement that we only know the list
// of connected clients when syncing, however getting the list of tabs could be called at anytime
// so we store it so we can translate from the tabs sync record ID to the FxA device id for the client
pub(crate) static REMOTE_CLIENTS_KEY: &str = "remote_clients";

pub struct TabsMigrationLogic;

impl MigrationLogic for TabsMigrationLogic {
    const NAME: &'static str = "tabs storage db";
    const END_VERSION: u32 = 2;

    fn prepare(&self, conn: &Connection, _db_empty: bool) -> MigrationResult<()> {
        let initial_pragmas = "
            -- We don't care about temp tables being persisted to disk.
            PRAGMA temp_store = 2;
            -- we unconditionally want write-ahead-logging mode.
            PRAGMA journal_mode=WAL;
            -- foreign keys seem worth enforcing (and again, we don't care in practice)
            PRAGMA foreign_keys = ON;
        ";
        conn.execute_batch(initial_pragmas)?;
        // This is where we'd define our sql functions if we had any!
        conn.set_prepared_statement_cache_capacity(128);
        Ok(())
    }

    fn init(&self, db: &Transaction<'_>) -> MigrationResult<()> {
        log::debug!("Creating schemas");
        db.execute_all(&[CREATE_SCHEMA_SQL, CREATE_META_TABLE_SQL])?;
        Ok(())
    }

    fn upgrade_from(&self, db: &Transaction<'_>, version: u32) -> MigrationResult<()> {
        match version {
            1 => upgrade_from_v1(db),
            _ => Err(MigrationError::IncompatibleVersion(version)),
        }
    }
}

fn upgrade_from_v1(db: &Connection) -> MigrationResult<()> {
    // The previous version stored the entire payload in one row
    // and cleared on each sync -- it's fine to just drop it
    db.execute_batch("DROP TABLE tabs;")?;
    // Recreate the world
    db.execute_all(&[CREATE_SCHEMA_SQL, CREATE_META_TABLE_SQL])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::TabsStorage;
    use rusqlite::OptionalExtension;
    use serde_json::json;
    use sql_support::open_database::test_utils::MigratedDatabaseFile;

    const CREATE_V1_SCHEMA_SQL: &str = "
        CREATE TABLE IF NOT EXISTS tabs (
            payload TEXT NOT NULL
        );
        PRAGMA user_version=1;
    ";

    #[test]
    fn test_create_schema_twice() {
        let mut db = TabsStorage::new_with_mem_path("test");
        let conn = db.open_or_create().unwrap();
        conn.execute_batch(CREATE_SCHEMA_SQL)
            .expect("should allow running twice");
    }

    #[test]
    fn test_tabs_db_upgrade_from_v1() {
        let db_file = MigratedDatabaseFile::new(TabsMigrationLogic, CREATE_V1_SCHEMA_SQL);
        db_file.run_all_upgrades();
        // Verify we can open the DB just fine, since migration is essentially a drop
        // we don't need to check any data integrity
        let mut storage = TabsStorage::new(db_file.path);
        storage.open_or_create().unwrap();
        assert!(storage.open_if_exists().unwrap().is_some());

        let test_payload = json!({
            "id": "device-with-a-tab",
            "clientName": "device with a tab",
            "tabs": [{
                "title": "the title",
                "urlHistory": [
                    "https://mozilla.org/"
                ],
                "icon": "https://mozilla.org/icon",
                "lastUsed": 1643764207,
            }]
        });
        let db = storage.open_if_exists().unwrap().unwrap();
        // We should be able to insert without a SQL error after upgrade
        db.execute(
            "INSERT INTO tabs (guid, record, last_modified) VALUES (:guid, :record, :last_modified);",
            rusqlite::named_params! {
                ":guid": "my-device",
                ":record": serde_json::to_string(&test_payload).unwrap(),
                ":last_modified": "1643764207"
            },
        )
        .unwrap();

        let row: Option<String> = db
            .query_row("SELECT guid FROM tabs;", [], |row| row.get(0))
            .optional()
            .unwrap();
        // Verify we can query for a valid guid now
        assert_eq!(row.unwrap(), "my-device");
    }
}

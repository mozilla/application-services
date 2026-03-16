/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use rusqlite::{Connection, Transaction};
use sql_support::open_database::{self, ConnectionInitializer};

/// The current gatabase schema version.
///
/// For any changes to the schema [`SQL`], please make sure to:
///
///  1. Bump this version.
///  2. Add a migration from the old version to the new version in
///     [`RemoteSettingsConnectionInitializer::upgrade_from`].
pub const VERSION: u32 = 2;

/// The current remote settings database schema.
pub const SQL: &str = r#"
CREATE TABLE IF NOT EXISTS records (
    id TEXT PRIMARY KEY,
    collection_url TEXT NOT NULL,
    data BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY,
    collection_url TEXT NOT NULL,
    data BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS collection_metadata (
    collection_url TEXT PRIMARY KEY,
    last_modified INTEGER, bucket TEXT, signatures TEXT);
"#;

/// Initializes an SQLite connection to the Remote Settings database, performing
/// migrations as needed.
#[derive(Default)]
pub struct RemoteSettingsConnectionInitializer;

impl ConnectionInitializer for RemoteSettingsConnectionInitializer {
    const NAME: &'static str = "remote_settings";
    const END_VERSION: u32 = 3;

    fn prepare(&self, conn: &Connection, _db_empty: bool) -> open_database::Result<()> {
        let initial_pragmas = "
            -- Use in-memory storage for TEMP tables.
            PRAGMA temp_store = 2;
            PRAGMA journal_mode = WAL;
        ";
        conn.execute_batch(initial_pragmas)?;
        sql_support::debug_tools::define_debug_functions(conn)?;

        Ok(())
    }

    fn init(&self, db: &Transaction<'_>) -> open_database::Result<()> {
        db.execute_batch(SQL)?;
        Ok(())
    }

    fn upgrade_from(&self, tx: &Transaction<'_>, version: u32) -> open_database::Result<()> {
        match version {
            // Upgrade from a database created before this crate used sql-support.
            0 => {
                tx.execute("ALTER TABLE collection_metadata DROP column fetched", ())?;
                Ok(())
            }
            1 => {
                tx.execute("ALTER TABLE collection_metadata ADD COLUMN bucket TEXT", ())?;
                tx.execute(
                    "ALTER TABLE collection_metadata ADD COLUMN signature TEXT",
                    (),
                )?;
                tx.execute("ALTER TABLE collection_metadata ADD COLUMN x5u TEXT", ())?;
                Ok(())
            }
            2 => {
                tx.execute(
                    "ALTER TABLE collection_metadata ADD COLUMN signatures TEXT",
                    (),
                )?;
                tx.execute(
                    r#"
                    UPDATE collection_metadata
                    SET signatures = CASE
                        -- Replace empty signatures with empty arrays.
                        WHEN COALESCE(signature, '') = '' OR COALESCE(x5u, '') = ''
                            THEN json_array()
                        -- Add the existing signature as array with one element.
                        ELSE json_array(
                            json_object(
                                'signature', signature,
                                'x5u', x5u
                            )
                        )
                    END
                    "#,
                    (),
                )?;
                tx.execute("ALTER TABLE collection_metadata DROP COLUMN signature", ())?;
                tx.execute("ALTER TABLE collection_metadata DROP COLUMN x5u", ())?;
                Ok(())
            }
            _ => Err(open_database::Error::IncompatibleVersion(version)),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use sql_support::open_database::test_utils::MigratedDatabaseFile;

    // Snapshot of the v0 schema.  We use this to test that we can migrate from there to the
    // current schema.
    const V0_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS records (
    id TEXT PRIMARY KEY,
    collection_url TEXT NOT NULL,
    data BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY,
    collection_url TEXT NOT NULL,
    data BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS collection_metadata (
    collection_url TEXT PRIMARY KEY,
    last_modified INTEGER,
    fetched BOOLEAN);
PRAGMA user_version=0;
"#;

    /// Test running all schema upgrades from V0, which was the first schema with a "real"
    /// migration.
    ///
    /// If an upgrade fails, then this test will fail with a panic.
    #[test]
    fn test_all_upgrades() {
        let db_file = MigratedDatabaseFile::new(RemoteSettingsConnectionInitializer, V0_SCHEMA);
        db_file.run_all_upgrades();
        db_file.assert_schema_matches_new_database();
    }
}

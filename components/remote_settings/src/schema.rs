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
    const END_VERSION: u32 = 4;

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
            3 => {
                // Clean up orphaned attachment blobs that are no longer referenced
                // by any current record. A bug (FXIOS-15181) caused these to accumulate over time,
                // leading to a database to grow to 1+ GB ( where the expected size was ~11 MB).
                tx.execute(
                    "DELETE FROM attachments
                    WHERE NOT EXISTS (
                        SELECT 1 FROM records
                        WHERE json_extract(records.data, '$.attachment.location') = attachments.id
                    )",
                    (),
                )?;
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

    #[test]
    fn test_2_to_3_signatures() {
        let db_file = MigratedDatabaseFile::new(RemoteSettingsConnectionInitializer, V0_SCHEMA);
        db_file.upgrade_to(2);
        let mut conn = db_file.open();
        let tx = conn.transaction().unwrap();
        tx.execute(
            "INSERT INTO collection_metadata (collection_url, last_modified, bucket, signature, x5u) VALUES (?, ?, ?, ?, ?)",
            ("a", 123, "main", "sig1", "uri1"),
        ).unwrap();
        tx.execute(
            "INSERT INTO collection_metadata (collection_url, last_modified, bucket, signature, x5u) VALUES (?, ?, ?, ?, ?)",
            ("b", 456, "main", "sig2", "uri2"),
        ).unwrap();
        tx.commit().unwrap();

        db_file.upgrade_to(3);

        let mut stmt = conn
            .prepare("SELECT signatures FROM collection_metadata WHERE collection_url = 'a'")
            .unwrap();
        let signatures1: String = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(signatures1, r#"[{"signature":"sig1","x5u":"uri1"}]"#);

        stmt = conn
            .prepare("SELECT signatures FROM collection_metadata WHERE collection_url = 'b'")
            .unwrap();
        let signatures2: String = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(signatures2, r#"[{"signature":"sig2","x5u":"uri2"}]"#)
    }
}

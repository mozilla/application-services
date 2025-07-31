/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::api::places_api::ConnectionType;
use crate::bookmark_sync::engine::LAST_SYNC_META_KEY;
use crate::error::debug;
use crate::storage::bookmarks::{
    bookmark_sync::create_synced_bookmark_roots, create_bookmark_roots,
};
use crate::types::SyncStatus;
use rusqlite::Connection;
use sql_support::ConnExt;

use super::db::{Pragma, PragmaGuard};

pub const VERSION: u32 = 18;

// Shared schema and temp tables for the read-write and Sync connections.
const CREATE_SHARED_SCHEMA_SQL: &str = include_str!("../../sql/create_shared_schema.sql");
const CREATE_SHARED_TEMP_TABLES_SQL: &str = include_str!("../../sql/create_shared_temp_tables.sql");

// Sync-specific temp tables and triggers.
const CREATE_SYNC_TEMP_TABLES_SQL: &str = include_str!("../../sql/create_sync_temp_tables.sql");
const CREATE_SYNC_TRIGGERS_SQL: &str = include_str!("../../sql/create_sync_triggers.sql");

// Triggers for the main read-write connection only.
const CREATE_MAIN_TRIGGERS_SQL: &str = include_str!("../../sql/create_main_triggers.sql");

lazy_static::lazy_static! {
    // Triggers for the read-write and Sync connections.
    static ref CREATE_SHARED_TRIGGERS_SQL: String = {
        format!(
            include_str!("../../sql/create_shared_triggers.sql"),
            increase_frecency_stats = update_origin_frecency_stats("+"),
            decrease_frecency_stats = update_origin_frecency_stats("-"),
        )
    };
}

// Keys in the moz_meta table.
pub(crate) static MOZ_META_KEY_ORIGIN_FRECENCY_COUNT: &str = "origin_frecency_count";
pub(crate) static MOZ_META_KEY_ORIGIN_FRECENCY_SUM: &str = "origin_frecency_sum";
pub(crate) static MOZ_META_KEY_ORIGIN_FRECENCY_SUM_OF_SQUARES: &str =
    "origin_frecency_sum_of_squares";

fn update_origin_frecency_stats(op: &str) -> String {
    format!(
        "
        INSERT OR REPLACE INTO moz_meta(key, value)
        SELECT
            '{frecency_count}',
            IFNULL((SELECT value FROM moz_meta WHERE key = '{frecency_count}'), 0)
                {op} CAST (frecency > 0 AS INT)
        FROM moz_origins WHERE prefix = OLD.prefix AND host = OLD.host
        UNION
        SELECT
            '{frecency_sum}',
            IFNULL((SELECT value FROM moz_meta WHERE key = '{frecency_sum}'), 0)
                {op} MAX(frecency, 0)
        FROM moz_origins WHERE prefix = OLD.prefix AND host = OLD.host
        UNION
        SELECT
            '{frecency_sum_of_squares}',
            IFNULL((SELECT value FROM moz_meta WHERE key = '{frecency_sum_of_squares}'), 0)
                {op} (MAX(frecency, 0) * MAX(frecency, 0))
        FROM moz_origins WHERE prefix = OLD.prefix AND host = OLD.host",
        op = op,
        frecency_count = MOZ_META_KEY_ORIGIN_FRECENCY_COUNT,
        frecency_sum = MOZ_META_KEY_ORIGIN_FRECENCY_SUM,
        frecency_sum_of_squares = MOZ_META_KEY_ORIGIN_FRECENCY_SUM_OF_SQUARES,
    )
}

pub fn init(conn: &Connection) -> rusqlite::Result<()> {
    debug!("Initializing schema");
    conn.execute_batch(CREATE_SHARED_SCHEMA_SQL)?;
    create_bookmark_roots(conn)?;
    Ok(())
}

pub fn finish(db: &Connection, conn_type: ConnectionType) -> rusqlite::Result<()> {
    match conn_type {
        // Read-only connections don't need temp tables or triggers, as they
        // can't write anything.
        ConnectionType::ReadOnly => {}

        // The main read-write connection needs shared and main-specific
        // temp tables and triggers (for example, for writing tombstones).
        ConnectionType::ReadWrite => {
            // Every writable connection needs these...
            db.execute_batch(CREATE_SHARED_TEMP_TABLES_SQL)?;
            db.execute_batch(&CREATE_SHARED_TRIGGERS_SQL)?;
            db.execute_batch(CREATE_MAIN_TRIGGERS_SQL)?;
        }

        // The Sync connection needs shared and its own temp tables and
        // triggers, for merging. It also bypasses some of the main
        // triggers, so that we don't write tombstones for synced deletions.
        ConnectionType::Sync => {
            db.execute_batch(CREATE_SHARED_TEMP_TABLES_SQL)?;
            db.execute_batch(&CREATE_SHARED_TRIGGERS_SQL)?;
            db.execute_batch(CREATE_SYNC_TEMP_TABLES_SQL)?;
            db.execute_batch(CREATE_SYNC_TRIGGERS_SQL)?;
            create_synced_bookmark_roots(db)?;
        }
    }
    Ok(())
}

/// Helper for migration - pre-dates MigrationLogic, hence it has slightly strange wiring.
/// Intended use:
///
/// ```rust,ignore
/// migration(db, cur_ver, 2, &[stuff, to, migrate, version2, to, version3], || Ok(()))?;
/// migration(db, cur_ver, 3, &[stuff, to, migrate, version3, to, version4], || Ok(()))?;
/// migration(db, cur_ver, 4, &[stuff, to, migrate, version4, to, version5], || Ok(()))?;
/// ```
///
/// The callback parameter is if any extra logic is needed for the migration
/// (for example, creating bookmark roots). In an ideal world, this would be an
/// Option, but sadly, that can't typecheck.
fn migration<F>(
    db: &Connection,
    cur_version: u32,
    ours: u32,
    stmts: &[&str],
    extra_logic: F,
) -> rusqlite::Result<()>
where
    F: FnOnce() -> rusqlite::Result<()>,
{
    if cur_version == ours {
        debug!("Upgrading schema from {} to {}", cur_version, ours);
        for stmt in stmts {
            db.execute_batch(stmt)?;
        }
        extra_logic()?;
    }
    Ok(())
}

pub fn upgrade_from(db: &Connection, from: u32) -> rusqlite::Result<()> {
    debug!("Upgrading schema from {} to {}", from, VERSION);

    // Old-style migrations

    migration(db, from, 2, &[CREATE_SHARED_SCHEMA_SQL], || Ok(()))?;
    migration(
        db,
        from,
        3,
        &[
            // Previous versions had an incomplete version of moz_bookmarks.
            "DROP TABLE moz_bookmarks",
            CREATE_SHARED_SCHEMA_SQL,
        ],
        || create_bookmark_roots(db.conn()),
    )?;
    migration(db, from, 4, &[CREATE_SHARED_SCHEMA_SQL], || Ok(()))?;
    migration(db, from, 5, &[CREATE_SHARED_SCHEMA_SQL], || Ok(()))?; // new tags tables.
    migration(db, from, 6, &[CREATE_SHARED_SCHEMA_SQL], || Ok(()))?; // bookmark syncing.
    migration(
        db,
        from,
        7,
        &[
            // Changing `moz_bookmarks_synced_structure` to store multiple
            // parents, so we need to re-download all synced bookmarks.
            &format!("DELETE FROM moz_meta WHERE key = '{}'", LAST_SYNC_META_KEY),
            "DROP TABLE moz_bookmarks_synced",
            "DROP TABLE moz_bookmarks_synced_structure",
            CREATE_SHARED_SCHEMA_SQL,
        ],
        || Ok(()),
    )?;
    migration(
        db,
        from,
        8,
        &[
            // Bump change counter of New() items due to bookmarks `reset`
            // setting the counter to 0 instead of 1 (#1145)
            &format!(
                "UPDATE moz_bookmarks
                 SET syncChangeCounter = syncChangeCounter + 1
                 WHERE syncStatus = {}",
                SyncStatus::New as u8
            ),
        ],
        || Ok(()),
    )?;
    migration(
        db,
        from,
        9,
        &[
            // Add an index for synced bookmark URLs.
            "CREATE INDEX IF NOT EXISTS moz_bookmarks_synced_urls
             ON moz_bookmarks_synced(placeId)",
        ],
        || Ok(()),
    )?;
    migration(
        db,
        from,
        10,
        &[
            // Add a new table to hold synced and migrated search keywords.
            "CREATE TABLE IF NOT EXISTS moz_keywords(
                 place_id INTEGER PRIMARY KEY REFERENCES moz_places(id)
                                  ON DELETE RESTRICT,
                 keyword TEXT NOT NULL UNIQUE
             )",
            // Add an index on synced keywords, so that we can search for
            // mismatched keywords without a table scan.
            "CREATE INDEX IF NOT EXISTS moz_bookmarks_synced_keywords
             ON moz_bookmarks_synced(keyword) WHERE keyword NOT NULL",
            // Migrate synced keywords into their own table, so that they're
            // available via `bookmarks_get_url_for_keyword` before the next
            // sync.
            "INSERT OR IGNORE INTO moz_keywords(keyword, place_id)
             SELECT keyword, placeId
             FROM moz_bookmarks_synced
             WHERE placeId NOT NULL AND
                   keyword NOT NULL",
        ],
        || Ok(()),
    )?;
    migration(
        db,
        from,
        11,
        &[
            // Greatly helps the multi-join query in frecency.
            "CREATE INDEX IF NOT EXISTS visits_from_type_idx
             ON moz_historyvisits(from_visit, visit_type)",
        ],
        || Ok(()),
    )?;
    migration(
        db,
        from,
        12,
        &[
            // Reconciled items didn't end up with the correct syncStatus.
            // See #3504
            "UPDATE moz_bookmarks AS b
             SET syncStatus = 2 -- SyncStatus::Normal
             WHERE EXISTS (SELECT 1 FROM moz_bookmarks_synced
                                    WHERE guid = b.guid)",
        ],
        || Ok(()),
    )?;
    migration(db, from, 13, &[CREATE_SHARED_SCHEMA_SQL], || Ok(()))?; // moz_places_metadata.
    migration(
        db,
        from,
        14,
        &[
            // Changing `moz_places_metadata` structure, drop and recreate it.
            "DROP TABLE moz_places_metadata",
            CREATE_SHARED_SCHEMA_SQL,
        ],
        || Ok(()),
    )?;

    // End of old style migrations, starting with the 15 -> 16 migration, we just use match
    // statements

    match from {
        // Skip the old style migrations
        n if n < 15 => (),
        // New-style migrations start here
        15 => {
            // Add the `unknownFields` column
            //
            // This migration was rolled out incorrectly and we need to check if it was already
            // applied (https://github.com/mozilla/application-services/issues/5464)
            let exists_sql = "SELECT 1 FROM pragma_table_info('moz_bookmarks_synced') WHERE name = 'unknownFields'";
            let add_column_sql = "ALTER TABLE moz_bookmarks_synced ADD COLUMN unknownFields TEXT";
            if !db.exists(exists_sql, [])? {
                db.execute(add_column_sql, [])?;
            }
        }
        16 => {
            // Add the `unknownFields` column for history
            db.execute("ALTER TABLE moz_places ADD COLUMN unknown_fields TEXT", ())?;
            db.execute(
                "ALTER TABLE moz_historyvisits ADD COLUMN unknown_fields TEXT",
                (),
            )?;
        }
        17 => {
            // Drop the CHECK and `FOREIGN KEY(parent)` constraints on
            // `moz_bookmarks`; schemas >= 18 enforce constraints using
            // TEMP triggers with more informative error messages.

            // SQLite doesn't support `ALTER TABLE DROP CONSTRAINT`, so
            // we rewrite the schema.

            const NEW_SQL: &str = "CREATE TABLE moz_bookmarks ( \
                id INTEGER PRIMARY KEY, \
                fk INTEGER DEFAULT NULL, \
                type INTEGER NOT NULL, \
                parent INTEGER, \
                position INTEGER NOT NULL, \
                title TEXT, \
                dateAdded INTEGER NOT NULL DEFAULT 0, \
                lastModified INTEGER NOT NULL DEFAULT 0, \
                guid TEXT NOT NULL UNIQUE, \
                syncStatus INTEGER NOT NULL DEFAULT 0, \
                syncChangeCounter INTEGER NOT NULL DEFAULT 1, \
                FOREIGN KEY(fk) REFERENCES moz_places(id) ON DELETE RESTRICT)";

            let _c = PragmaGuard::new(db, Pragma::IgnoreCheckConstraints, true)?;
            let _f = PragmaGuard::new(db, Pragma::ForeignKeys, false)?;
            let _w = PragmaGuard::new(db, Pragma::WritableSchema, true)?;

            db.execute(
                "UPDATE sqlite_schema SET
                   sql = ?
                 WHERE type = 'table' AND name = 'moz_bookmarks'",
                // _Must_ be valid SQL; updating `sqlite_schema.sql` with
                // invalid SQL will corrupt the database.
                rusqlite::params![NEW_SQL],
            )?;
        }
        // Add more migrations here...

        // Any other from value indicates that something very wrong happened
        _ => panic!(
            "Places does not have a v{} -> v{} migration",
            from,
            from + 1
        ),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{db::PlacesInitializer, PlacesDb};
    use crate::error::Result;
    use sql_support::open_database::test_utils::MigratedDatabaseFile;
    use std::collections::BTreeSet;
    use sync_guid::Guid as SyncGuid;
    use url::Url;

    #[test]
    fn test_create_schema_twice() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("no memory db");
        conn.execute_batch(CREATE_SHARED_SCHEMA_SQL)
            .expect("should allow running twice");
    }

    fn has_tombstone(conn: &PlacesDb, guid: &SyncGuid) -> bool {
        let count: rusqlite::Result<Option<u32>> = conn.try_query_row(
            "SELECT COUNT(*) from moz_places_tombstones
                     WHERE guid = :guid",
            &[(":guid", guid)],
            |row| row.get::<_, u32>(0),
            true,
        );
        count.unwrap().unwrap() == 1
    }

    #[test]
    fn test_places_no_tombstone() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("no memory db");
        let guid = SyncGuid::random();

        conn.execute_cached(
            "INSERT INTO moz_places (guid, url, url_hash) VALUES (:guid, :url, hash(:url))",
            &[
                (":guid", &guid as &dyn rusqlite::ToSql),
                (
                    ":url",
                    &String::from(Url::parse("http://example.com").expect("valid url")),
                ),
            ],
        )
        .expect("should work");

        let place_id = conn.last_insert_rowid();
        conn.execute_cached(
            "DELETE FROM moz_places WHERE id = :id",
            &[(":id", &place_id)],
        )
        .expect("should work");

        // should not have a tombstone.
        assert!(!has_tombstone(&conn, &guid));
    }

    #[test]
    fn test_places_tombstone_removal() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("no memory db");
        let guid = SyncGuid::random();

        conn.execute_cached(
            "INSERT INTO moz_places_tombstones VALUES (:guid)",
            &[(":guid", &guid)],
        )
        .expect("should work");

        // insert into moz_places - the tombstone should be removed.
        conn.execute_cached(
            "INSERT INTO moz_places (guid, url, url_hash, sync_status)
             VALUES (:guid, :url, hash(:url), :sync_status)",
            &[
                (":guid", &guid as &dyn rusqlite::ToSql),
                (
                    ":url",
                    &String::from(Url::parse("http://example.com").expect("valid url")),
                ),
                (":sync_status", &SyncStatus::Normal),
            ],
        )
        .expect("should work");
        assert!(!has_tombstone(&conn, &guid));
    }

    #[test]
    fn test_bookmark_check_constraints() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("no memory db");

        conn.execute_batch(
            "INSERT INTO moz_places(id, guid, url, frecency)
             VALUES(1, 'page_guid___', 'https://example.com', -1);",
        )
        .expect("should insert page");

        // type==BOOKMARK but null fk
        {
            let e = conn
                .execute_cached(
                    "INSERT INTO moz_bookmarks
                        (fk, type, parent, position, dateAdded, lastModified, guid)
                     VALUES
                        (NULL, 1, 0, 0, 1, 1, 'fake_guid___')",
                    [],
                )
                .expect_err("should fail to insert bookmark with NULL fk");
            assert!(
                e.to_string().contains("insert: type=1; fk NULL"),
                "Expected error, got: {:?}",
                e,
            );

            conn.execute_batch(
                "INSERT INTO moz_bookmarks
                    (fk, type, parent, position, dateAdded, lastModified,
                       guid)
                 VALUES
                    (1, 1, (SELECT id FROM moz_bookmarks WHERE guid = 'root________'), 0, 1, 1,
                       'bmk_guid____')",
            )
            .expect("should insert bookmark");
            let e = conn
                .execute(
                    "UPDATE moz_bookmarks SET
                        fk = NULL
                     WHERE guid = 'bmk_guid____'",
                    [],
                )
                .expect_err("should fail to update bookmark with NULL fk");
            assert!(
                e.to_string().contains("update: type=1; fk NULL"),
                "Expected error, got: {:?}",
                e,
            );
        }

        // type!=BOOKMARK and non-null fk
        {
            let e = conn
                .execute_cached(
                    "INSERT INTO moz_bookmarks
                        (fk, type, parent, position, dateAdded, lastModified, guid)
                     VALUES
                        (1, 2, 0, 0, 1, 1, 'fake_guid___')",
                    [],
                )
                .expect_err("should fail to insert folder with non-NULL fk");
            assert!(
                e.to_string().contains("insert: type=2; fk NOT NULL"),
                "Expected error, got: {:?}",
                e,
            );

            conn.execute_batch(
                "INSERT INTO moz_bookmarks
                    (fk, type, parent, position, dateAdded, lastModified,
                       guid)
                 VALUES
                    (NULL, 2, (SELECT id FROM moz_bookmarks WHERE guid = 'root________'), 1, 1, 1,
                       'folder_guid_')",
            )
            .expect("should insert folder");
            let e = conn
                .execute(
                    "UPDATE moz_bookmarks SET
                        fk = 1
                     WHERE guid = 'folder_guid_'",
                    [],
                )
                .expect_err("should fail to update folder with non-NULL fk");
            assert!(
                e.to_string().contains("update: type=2; fk NOT NULL"),
                "Expected error, got: {:?}",
                e,
            );
        }

        // null parent for item other than the root
        {
            let e = conn
                .execute_cached(
                    "INSERT INTO moz_bookmarks
                        (fk, type, parent, position, dateAdded, lastModified, guid)
                     VALUES
                        (NULL, 2, NULL, 0, 1, 1, 'fake_guid___')",
                    [],
                )
                .expect_err("should fail to insert item with NULL parent");
            assert!(
                e.to_string().contains("insert: item without parent"),
                "Expected error, got: {:?}",
                e,
            );

            let e = conn
                .execute_cached(
                    "INSERT INTO moz_bookmarks
                        (fk, type, parent, position, dateAdded, lastModified, guid)
                     VALUES
                        (NULL, 2, -1, 0, 1, 1, 'fake_guid___')",
                    [],
                )
                .expect_err("should fail to insert item with nonexistent parent");
            assert!(
                e.to_string().contains("insert: item without parent"),
                "Expected error, got: {:?}",
                e,
            );

            let e = conn
                .execute(
                    "UPDATE moz_bookmarks SET
                        parent = NULL
                     WHERE guid = 'folder_guid_'",
                    [],
                )
                .expect_err("should fail to update folder with NULL parent");
            assert!(
                e.to_string().contains("update: item without parent"),
                "Expected error, got: {:?}",
                e,
            );

            // Bug 1941655 - we only guard against NULL parents, not missing ones.
            /*
            let e = conn
                .execute(
                    "UPDATE moz_bookmarks SET
                        parent = -1
                     WHERE guid = 'folder_guid_'",
                    [],
                )
                .expect_err("should fail to update folder with nonexistent parent");
            assert!(
                e.to_string().contains("update: item without parent"),
                "Expected error, got: {:?}",
                e,
            );
            */
        }

        // Invalid length guid
        {
            let e = conn
                .execute_cached(
                    "INSERT INTO moz_bookmarks
                        (fk, type, parent, position, dateAdded, lastModified, guid)
                     VALUES
                        (NULL, 2, 0, 0, 1, 1, 'fake_guid')",
                    [],
                )
                .expect_err("should fail");
            assert!(
                e.to_string().contains("insert: len(guid)=9"),
                "Expected error, got: {:?}",
                e,
            );

            let e = conn
                .execute(
                    "UPDATE moz_bookmarks SET
                        guid = 'fake_guid'
                     WHERE guid = 'bmk_guid____'",
                    [],
                )
                .expect_err("should fail to update bookmark with invalid guid");
            assert!(
                e.to_string().contains("update: len(guid)=9"),
                "Expected error, got: {:?}",
                e,
            );
        }

        // Changing the type of an existing item.
        {
            let e = conn
                .execute(
                    "UPDATE moz_bookmarks SET
                        type = 3
                     WHERE guid = 'folder_guid_'",
                    [],
                )
                .expect_err("should fail to update type of bookmark");
            assert!(
                e.to_string().contains("update: old type=2; new=3"),
                "Expected error, got: {:?}",
                e,
            );
        }
    }

    fn select_simple_int(conn: &PlacesDb, stmt: &str) -> u32 {
        let count: Result<Option<u32>> =
            conn.try_query_row(stmt, [], |row| Ok(row.get::<_, u32>(0)?), false);
        count.unwrap().unwrap()
    }

    fn get_foreign_count(conn: &PlacesDb, guid: &SyncGuid) -> u32 {
        let count: Result<Option<u32>> = conn.try_query_row(
            "SELECT foreign_count from moz_places
                     WHERE guid = :guid",
            &[(":guid", guid)],
            |row| Ok(row.get::<_, u32>(0)?),
            true,
        );
        count.unwrap().unwrap()
    }

    #[test]
    fn test_bookmark_foreign_count_triggers() {
        // create the place.
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("no memory db");
        let guid1 = SyncGuid::random();
        let url1 = Url::parse("http://example.com").expect("valid url");
        let guid2 = SyncGuid::random();
        let url2 = Url::parse("http://example2.com").expect("valid url");

        conn.execute_cached(
            "INSERT INTO moz_places (guid, url, url_hash) VALUES (:guid, :url, hash(:url))",
            &[
                (":guid", &guid1 as &dyn rusqlite::ToSql),
                (":url", &String::from(url1)),
            ],
        )
        .expect("should work");
        let place_id1 = conn.last_insert_rowid();

        conn.execute_cached(
            "INSERT INTO moz_places (guid, url, url_hash) VALUES (:guid, :url, hash(:url))",
            &[
                (":guid", &guid2 as &dyn rusqlite::ToSql),
                (":url", &String::from(url2)),
            ],
        )
        .expect("should work");
        let place_id2 = conn.last_insert_rowid();

        assert_eq!(get_foreign_count(&conn, &guid1), 0);
        assert_eq!(get_foreign_count(&conn, &guid2), 0);

        // record visits for both URLs, otherwise the place itself will be removed with the bookmark
        conn.execute_cached(
            "INSERT INTO moz_historyvisits (place_id, visit_date, visit_type, is_local)
             VALUES (:place, 10000000, 1, 0);",
            &[(":place", &place_id1)],
        )
        .expect("should work");
        conn.execute_cached(
            "INSERT INTO moz_historyvisits (place_id, visit_date, visit_type, is_local)
             VALUES (:place, 10000000, 1, 1);",
            &[(":place", &place_id2)],
        )
        .expect("should work");

        // create a bookmark pointing at it.
        conn.execute_cached(
            "INSERT INTO moz_bookmarks
                (fk, type, parent, position, dateAdded, lastModified, guid)
            VALUES
                (:place_id, 1, 1, 0, 1, 1, 'fake_guid___')",
            &[(":place_id", &place_id1)],
        )
        .expect("should work");
        assert_eq!(get_foreign_count(&conn, &guid1), 1);
        assert_eq!(get_foreign_count(&conn, &guid2), 0);

        // change the bookmark to point at a different place.
        conn.execute_cached(
            "UPDATE moz_bookmarks SET fk = :new_place WHERE guid = 'fake_guid___';",
            &[(":new_place", &place_id2)],
        )
        .expect("should work");
        assert_eq!(get_foreign_count(&conn, &guid1), 0);
        assert_eq!(get_foreign_count(&conn, &guid2), 1);

        conn.execute("DELETE FROM moz_bookmarks WHERE guid = 'fake_guid___';", [])
            .expect("should work");
        assert_eq!(get_foreign_count(&conn, &guid1), 0);
        assert_eq!(get_foreign_count(&conn, &guid2), 0);
    }

    #[test]
    fn test_bookmark_synced_foreign_count_triggers() {
        // create the place.
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("no memory db");

        let url = Url::parse("http://example.com").expect("valid url");

        conn.execute_cached(
            "INSERT INTO moz_places (guid, url, url_hash) VALUES ('fake_guid___', :url, hash(:url))",
            &[(":url", &String::from(url))],
        )
        .expect("should work");
        let place_id = conn.last_insert_rowid();

        assert_eq!(get_foreign_count(&conn, &"fake_guid___".into()), 0);

        // create a bookmark pointing at it.
        conn.execute_cached(
            "INSERT INTO moz_bookmarks_synced
                (placeId, guid)
            VALUES
                (:place_id, 'fake_guid___')",
            &[(":place_id", &place_id)],
        )
        .expect("should work");
        assert_eq!(get_foreign_count(&conn, &"fake_guid___".into()), 1);

        // delete it.
        conn.execute_cached(
            "DELETE FROM moz_bookmarks_synced WHERE guid = 'fake_guid___';",
            [],
        )
        .expect("should work");
        assert_eq!(get_foreign_count(&conn, &"fake_guid___".into()), 0);
    }

    #[test]
    fn test_bookmark_delete_restrict() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("no memory db");
        conn.execute_all(&[
            "INSERT INTO moz_places
                (guid, url, url_hash)
             VALUES
                ('place_guid__', 'http://example.com/', hash('http://example.com/'))",
            "INSERT INTO moz_bookmarks
                (type, parent, position, dateAdded, lastModified, guid, fk)
            VALUES
                (1, 1, 0, 1, 1, 'fake_guid___', last_insert_rowid())",
        ])
        .expect("should be able to do the inserts");

        // Should be impossible to delete the place.
        conn.execute("DELETE FROM moz_places WHERE guid = 'place_guid__';", [])
            .expect_err("should fail");

        // delete the bookmark.
        conn.execute("DELETE FROM moz_bookmarks WHERE guid = 'fake_guid___';", [])
            .expect("should be able to delete the bookmark");

        // now we should be able to delete the place.
        conn.execute("DELETE FROM moz_places WHERE guid = 'place_guid__';", [])
            .expect("should now be able to delete the place");
    }

    #[test]
    fn test_bookmark_auto_deletes() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("no memory db");

        conn.execute_all(&[
            // A folder to hold a bookmark.
            "INSERT INTO moz_bookmarks
                (type, parent, position, dateAdded, lastModified, guid)
            VALUES
                (3, 1, 0, 1, 1, 'folder_guid_')",
            // A place for the bookmark.
            "INSERT INTO moz_places
                (guid, url, url_hash)
            VALUES ('place_guid__', 'http://example.com/', hash('http://example.com/'))",
            // The bookmark.
            "INSERT INTO moz_bookmarks
                (fk, type, parent, position, dateAdded, lastModified, guid)
            VALUES
                --fk,                  type
                (last_insert_rowid(), 1,
                -- parent
                 (SELECT id FROM moz_bookmarks WHERE guid = 'folder_guid_'),
                -- positon, dateAdded, lastModified, guid
                   0,       1,         1,           'bookmarkguid')",
        ])
        .expect("inserts should work");

        // Delete the folder - the bookmark should cascade delete.
        conn.execute("DELETE FROM moz_bookmarks WHERE guid = 'folder_guid_';", [])
            .expect("should work");

        // folder should be gone.
        assert_eq!(
            select_simple_int(
                &conn,
                "SELECT count(*) FROM moz_bookmarks WHERE guid = 'folder_guid_'"
            ),
            0
        );
        // bookmark should be gone.
        assert_eq!(
            select_simple_int(
                &conn,
                "SELECT count(*) FROM moz_bookmarks WHERE guid = 'bookmarkguid';"
            ),
            0
        );

        // Place should also be gone as bookmark url had no visits.
        assert_eq!(
            select_simple_int(
                &conn,
                "SELECT COUNT(*) from moz_places WHERE guid = 'place_guid__';"
            ),
            0
        );
    }

    #[test]
    fn test_bookmark_auto_deletes_place_remains() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("no memory db");

        conn.execute_all(&[
            // A folder to hold a bookmark.
            "INSERT INTO moz_bookmarks
                (type, parent, position, dateAdded, lastModified, guid)
            VALUES
                (3, 1, 0, 1, 1, 'folder_guid_')",
            // A place for the bookmark.
            "INSERT INTO moz_places
                (guid, url, url_hash, foreign_count) -- here we pretend it has a foreign count.
            VALUES ('place_guid__', 'http://example.com/', hash('http://example.com/'), 1)",
            // The bookmark.
            "INSERT INTO moz_bookmarks
                (fk, type, parent, position, dateAdded, lastModified, guid)
            VALUES
                --fk,                  type
                (last_insert_rowid(), 1,
                -- parent
                 (SELECT id FROM moz_bookmarks WHERE guid = 'folder_guid_'),
                -- positon, dateAdded, lastModified, guid
                   0,       1,         1,           'bookmarkguid')",
        ])
        .expect("inserts should work");

        // Delete the folder - the bookmark should cascade delete.
        conn.execute("DELETE FROM moz_bookmarks WHERE guid = 'folder_guid_';", [])
            .expect("should work");

        // folder should be gone.
        assert_eq!(
            select_simple_int(
                &conn,
                "SELECT count(*) FROM moz_bookmarks WHERE guid = 'folder_guid_'"
            ),
            0
        );
        // bookmark should be gone.
        assert_eq!(
            select_simple_int(
                &conn,
                "SELECT count(*) FROM moz_bookmarks WHERE guid = 'bookmarkguid';"
            ),
            0
        );

        // Place should remain as we pretended it has a foreign reference.
        assert_eq!(
            select_simple_int(
                &conn,
                "SELECT COUNT(*) from moz_places WHERE guid = 'place_guid__';"
            ),
            1
        );
    }

    #[test]
    fn test_bookmark_tombstone_auto_created() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("no memory db");
        conn.execute(
            &format!(
                "INSERT INTO moz_bookmarks
                        (syncStatus, type, parent, position, dateAdded, lastModified, guid)
                     VALUES
                        ({}, 3, 1, 0, 1, 1, 'bookmarkguid')",
                SyncStatus::Normal as u8
            ),
            [],
        )
        .expect("should insert regular bookmark folder");
        conn.execute("DELETE FROM moz_bookmarks WHERE guid = 'bookmarkguid'", [])
            .expect("should delete");
        // should have a tombstone.
        assert_eq!(
            select_simple_int(&conn, "SELECT COUNT(*) from moz_bookmarks_deleted"),
            1
        );
        conn.execute("DELETE from moz_bookmarks_deleted", [])
            .expect("should delete");
        conn.execute(
            &format!(
                "INSERT INTO moz_bookmarks
                        (syncStatus, type, parent, position, dateAdded, lastModified, guid)
                     VALUES
                        ({}, 3, 1, 0, 1, 1, 'bookmarkguid')",
                SyncStatus::New as u8
            ),
            [],
        )
        .expect("should insert regular bookmark folder");
        conn.execute("DELETE FROM moz_bookmarks WHERE guid = 'bookmarkguid'", [])
            .expect("should delete");
        // should not have a tombstone as syncStatus is new.
        assert_eq!(
            select_simple_int(&conn, "SELECT COUNT(*) from moz_bookmarks_deleted"),
            0
        );
    }

    #[test]
    fn test_bookmark_tombstone_auto_deletes() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("no memory db");
        conn.execute(
            "INSERT into moz_bookmarks_deleted VALUES ('bookmarkguid', 1)",
            [],
        )
        .expect("should insert tombstone");
        assert_eq!(
            select_simple_int(&conn, "SELECT COUNT(*) from moz_bookmarks_deleted"),
            1
        );
        // create a bookmark with the same guid.
        conn.execute(
            "INSERT INTO moz_bookmarks
                        (type, parent, position, dateAdded, lastModified, guid)
                     VALUES
                        (3, 1, 0, 1, 1, 'bookmarkguid')",
            [],
        )
        .expect("should insert regular bookmark folder");
        // tombstone should have vanished.
        assert_eq!(
            select_simple_int(&conn, "SELECT COUNT(*) from moz_bookmarks_deleted"),
            0
        );
    }

    #[test]
    fn test_bookmark_tombstone_auto_deletes_on_update() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("no memory db");

        // check updates do the right thing.
        conn.execute(
            "INSERT into moz_bookmarks_deleted VALUES ('bookmarkguid', 1)",
            [],
        )
        .expect("should insert tombstone");

        // create a bookmark with a different guid.
        conn.execute(
            "INSERT INTO moz_bookmarks
                        (type, parent, position, dateAdded, lastModified, guid)
                     VALUES
                        (3, 1, 0, 1, 1, 'fake_guid___')",
            [],
        )
        .expect("should insert regular bookmark folder");
        // tombstone should remain.
        assert_eq!(
            select_simple_int(&conn, "SELECT COUNT(*) from moz_bookmarks_deleted"),
            1
        );
        // update guid - should fail as we have a trigger with RAISEs.
        conn.execute(
            "UPDATE moz_bookmarks SET guid = 'bookmarkguid'
             WHERE guid = 'fake_guid___'",
            [],
        )
        .expect_err("changing the guid should fail");
    }

    #[test]
    fn test_origin_triggers_simple_removal() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("no memory db");
        let guid = SyncGuid::random();
        let url = String::from(Url::parse("http://example.com").expect("valid url"));

        conn.execute(
            "INSERT INTO moz_places (guid, url, url_hash) VALUES (:guid, :url, hash(:url))",
            rusqlite::named_params! {
                ":guid": &guid,
                ":url": &url,
            },
        )
        .expect("should work");
        // origins are maintained via triggers, so make sure they are done.
        crate::storage::delete_pending_temp_tables(&conn).expect("should work");

        // We should have inserted the origin.
        assert_eq!(
            select_simple_int(
                &conn,
                "SELECT count(*) FROM moz_origins WHERE host = 'example.com'"
            ),
            1
        );

        // delete the place, ensure triggers have run, and check origin has gone.
        conn.execute("DELETE FROM moz_places", [])
            .expect("should work");
        crate::storage::delete_pending_temp_tables(&conn).expect("should work");
        assert_eq!(
            select_simple_int(&conn, "SELECT count(*) FROM moz_origins"),
            0
        );
    }

    const CREATE_V15_DB: &str = include_str!("../../sql/tests/create_v15_db.sql");

    #[test]
    fn test_upgrade_schema_15_16() {
        let db_file = MigratedDatabaseFile::new(PlacesInitializer::new_for_test(), CREATE_V15_DB);

        db_file.upgrade_to(16);
        let db = db_file.open();

        // Test the unknownFields column was added
        assert_eq!(
            db.query_one::<String>("SELECT type FROM pragma_table_info('moz_bookmarks_synced') WHERE name = 'unknownFields'").unwrap(),
            "TEXT"
        );
    }

    #[test]
    fn test_gh5464() {
        // Test the gh-5464 error case: A user with the `v16` schema, but with `user_version` set
        // to 15
        let db_file = MigratedDatabaseFile::new(PlacesInitializer::new_for_test(), CREATE_V15_DB);
        db_file.upgrade_to(16);
        let db = db_file.open();
        db.execute("PRAGMA user_version=15", []).unwrap();
        drop(db);
        db_file.upgrade_to(16);
    }

    const CREATE_V17_DB: &str = include_str!("../../sql/tests/create_v17_db.sql");

    #[test]
    fn test_upgrade_schema_17_18() {
        let db_file = MigratedDatabaseFile::new(PlacesInitializer::new_for_test(), CREATE_V17_DB);

        db_file.upgrade_to(18);
        let db = db_file.open();

        let sql = db
            .query_one::<String>(
                "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'moz_bookmarks'",
            )
            .expect("should retrieve CREATE TABLE statement");
        assert!(!sql.contains("CHECK"));
        assert!(!sql.contains("FOREIGN KEY(parent)"));

        // Make sure that we didn't corrupt the database.
        let integrity_ok = db
            .query_row("PRAGMA integrity_check", [], |row| {
                Ok(row.get::<_, String>(0)? == "ok")
            })
            .expect("should perform integrity check");
        assert!(integrity_ok);

        let foreign_keys_ok = db
            .prepare("PRAGMA foreign_key_check")
            .and_then(|mut statement| Ok(statement.query([])?.next()?.is_none()))
            .expect("should perform foreign key check");
        assert!(foreign_keys_ok);

        // ...And that we can read everything we inserted.
        #[derive(Eq, PartialEq, Debug)]
        struct BookmarkRow {
            id: i64,
            type_: i64,
            parent: Option<i64>,
            fk: Option<i64>,
            guid: String,
        }
        let rows = db
            .query_rows_and_then(
                "SELECT id, type, parent, fk, guid FROM moz_bookmarks ORDER BY id",
                [],
                |row| -> rusqlite::Result<_> {
                    Ok(BookmarkRow {
                        id: row.get("id")?,
                        type_: row.get("type")?,
                        parent: row.get("parent")?,
                        fk: row.get("fk")?,
                        guid: row.get("guid")?,
                    })
                },
            )
            .expect("should query all bookmark rows");
        assert_eq!(
            rows,
            &[
                BookmarkRow {
                    id: 1,
                    type_: 2,
                    parent: None,
                    fk: None,
                    guid: "root________".into()
                },
                BookmarkRow {
                    id: 2,
                    type_: 2,
                    parent: Some(1),
                    fk: None,
                    guid: "folder_guid_".into()
                },
                BookmarkRow {
                    id: 3,
                    type_: 1,
                    parent: Some(2),
                    fk: Some(1),
                    guid: "bmk_guid____".into()
                },
                BookmarkRow {
                    id: 4,
                    type_: 3,
                    parent: Some(2),
                    fk: None,
                    guid: "sep_guid____".into()
                }
            ]
        );
    }

    #[test]
    fn test_all_upgrades() {
        // Test the migration process in general: open a fresh DB and a DB that's gone through the migration
        // process.  Check that the schemas match.
        let fresh_db = PlacesDb::open_in_memory(ConnectionType::ReadWrite).unwrap();

        let db_file = MigratedDatabaseFile::new(PlacesInitializer::new_for_test(), CREATE_V15_DB);
        db_file.run_all_upgrades();
        let upgraded_db = db_file.open();

        assert_eq!(
            fresh_db.query_one::<u32>("PRAGMA user_version").unwrap(),
            upgraded_db.query_one::<u32>("PRAGMA user_version").unwrap(),
        );
        let all_tables = [
            "moz_places",
            "moz_places_tombstones",
            "moz_places_stale_frecencies",
            "moz_historyvisits",
            "moz_historyvisit_tombstones",
            "moz_inputhistory",
            "moz_bookmarks",
            "moz_bookmarks_deleted",
            "moz_origins",
            "moz_meta",
            "moz_tags",
            "moz_tags_relation",
            "moz_bookmarks_synced",
            "moz_bookmarks_synced_structure",
            "moz_bookmarks_synced_tag_relation",
            "moz_keywords",
            "moz_places_metadata",
            "moz_places_metadata_search_queries",
        ];
        #[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
        struct ColumnInfo {
            name: String,
            type_: String,
            not_null: bool,
            default_value: Option<String>,
            pk: bool,
        }

        fn get_table_column_info(conn: &Connection, table_name: &str) -> BTreeSet<ColumnInfo> {
            let mut stmt = conn
                .prepare("SELECT name, type, `notnull`, dflt_value, pk FROM pragma_table_info(?)")
                .unwrap();
            stmt.query_map((table_name,), |row| {
                Ok(ColumnInfo {
                    name: row.get(0)?,
                    type_: row.get(1)?,
                    not_null: row.get(2)?,
                    default_value: row.get(3)?,
                    pk: row.get(4)?,
                })
            })
            .unwrap()
            .collect::<rusqlite::Result<BTreeSet<_>>>()
            .unwrap()
        }
        for table_name in all_tables {
            assert_eq!(
                get_table_column_info(&upgraded_db, table_name),
                get_table_column_info(&fresh_db, table_name),
            );
        }
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// XXXXXX - This has been cloned from logins/src/schema.rs, on Thom's
// wip-sync-sql-store branch.
// We should work out how to turn this into something that can use a shared
// db.rs.

use crate::db::PlacesDb;
use crate::error::*;
use crate::storage::bookmarks::create_bookmark_roots;
use lazy_static::lazy_static;
use rusqlite::NO_PARAMS;
use sql_support::ConnExt;

const VERSION: i64 = 3;

const CREATE_TABLE_PLACES_SQL: &str =
    "CREATE TABLE IF NOT EXISTS moz_places (
        id INTEGER PRIMARY KEY,
        url LONGVARCHAR NOT NULL,
        title LONGVARCHAR,
        -- note - desktop has rev_host here - that's now in moz_origin.
        visit_count_local INTEGER NOT NULL DEFAULT 0,
        visit_count_remote INTEGER NOT NULL DEFAULT 0,
        hidden INTEGER DEFAULT 0 NOT NULL,
        typed INTEGER DEFAULT 0 NOT NULL, -- XXX - is 'typed' ok? Note also we want this as a *count*, not a bool.
        frecency INTEGER DEFAULT -1 NOT NULL,
        -- XXX - splitting last visit into local and remote correct?
        last_visit_date_local INTEGER NOT NULL DEFAULT 0,
        last_visit_date_remote INTEGER NOT NULL DEFAULT 0,
        guid TEXT NOT NULL UNIQUE,
        foreign_count INTEGER DEFAULT 0 NOT NULL,
        url_hash INTEGER DEFAULT 0 NOT NULL,
        description TEXT, -- XXXX - title above?
        preview_image_url TEXT,
        -- origin_id would ideally be NOT NULL, but we use a trigger to keep
        -- it up to date, so do perform the initial insert with a null.
        origin_id INTEGER,
        -- a couple of sync-related fields.
        sync_status TINYINT NOT NULL DEFAULT 1, -- 1 is SyncStatus::New
        sync_change_counter INTEGER NOT NULL DEFAULT 0, -- adding visits will increment this

        FOREIGN KEY(origin_id) REFERENCES moz_origins(id) ON DELETE CASCADE
    )";

const CREATE_TABLE_PLACES_TOMBSTONES_SQL: &str =
    "CREATE TABLE IF NOT EXISTS moz_places_tombstones (
        guid TEXT PRIMARY KEY
    ) WITHOUT ROWID";

const CREATE_TABLE_HISTORYVISITS_SQL: &str =
    "CREATE TABLE moz_historyvisits (
        id INTEGER PRIMARY KEY,
        is_local INTEGER NOT NULL, -- XXX - not in desktop - will always be true for visits added locally, always false visits added by sync.
        from_visit INTEGER, -- XXX - self-reference?
        place_id INTEGER NOT NULL,
        visit_date INTEGER NOT NULL,
        visit_type INTEGER NOT NULL,
        -- session INTEGER, -- XXX - what is 'session'? Appears unused.

        FOREIGN KEY(place_id) REFERENCES moz_places(id) ON DELETE CASCADE,
        FOREIGN KEY(from_visit) REFERENCES moz_historyvisits(id)
    )";

const CREATE_TABLE_INPUTHISTORY_SQL: &str = "CREATE TABLE moz_inputhistory (
        place_id INTEGER NOT NULL,
        input LONGVARCHAR NOT NULL,
        use_count INTEGER,

        PRIMARY KEY (place_id, input),
        FOREIGN KEY(place_id) REFERENCES moz_places(id) ON DELETE CASCADE
    )";

// XXX - TODO - moz_annos
// XXX - TODO - moz_anno_attributes
// XXX - TODO - moz_items_annos

const CREATE_TABLE_BOOKMARKS_SQL: &str = r#"CREATE TABLE moz_bookmarks (
        id INTEGER PRIMARY KEY,
        fk INTEGER DEFAULT NULL, -- place_id
        type INTEGER NOT NULL CHECK(type BETWEEN 1 AND 3),
        parent INTEGER,
        position INTEGER NOT NULL CHECK(position >= 0),
        title TEXT, -- a'la bug 1356159, NULL is special here - it means 'not edited'
        dateAdded INTEGER NOT NULL DEFAULT 0,
        lastModified INTEGER NOT NULL DEFAULT 0,
        guid TEXT NOT NULL UNIQUE CHECK(length(guid) == 12),

        syncStatus INTEGER NOT NULL DEFAULT 0 CHECK(syncStatus BETWEEN 0 AND 2),
        syncChangeCounter INTEGER NOT NULL DEFAULT 1,

        -- bookmarks must have a fk to a URL, other types must not.
        CHECK((type == 1 AND fk IS NOT NULL) OR (type > 1 AND fk IS NULL))
        -- only the root is allowed to have a non-null parent
        CHECK(guid == "root________" OR parent IS NOT NULL)

        FOREIGN KEY(fk) REFERENCES moz_places(id) ON DELETE RESTRICT
        FOREIGN KEY(parent) REFERENCES moz_bookmarks(id) ON DELETE CASCADE
    )"#;

const CREATE_TABLE_BOOKMARKS_DELETED_SQL: &str = "CREATE TABLE moz_bookmarks_deleted (
        guid TEXT PRIMARY KEY,
        dateRemoved INTEGER NOT NULL
    ) WITHOUT ROWID";
// Note: desktop has/had a 'keywords' table, but we intentionally do not.

const CREATE_TABLE_ORIGINS_SQL: &str = "CREATE TABLE moz_origins (
        id INTEGER PRIMARY KEY,
        prefix TEXT NOT NULL,
        host TEXT NOT NULL,
        rev_host TEXT NOT NULL,
        frecency INTEGER NOT NULL, -- XXX - why not default of -1 like in moz_places?
        UNIQUE (prefix, host)
    )";

const CREATE_TRIGGER_AFTER_INSERT_ON_PLACES: &str = "
    CREATE TEMP TRIGGER moz_places_afterinsert_trigger
    AFTER INSERT ON moz_places FOR EACH ROW
    BEGIN
        INSERT OR IGNORE INTO moz_origins(prefix, host, rev_host, frecency)
        VALUES(get_prefix(NEW.url), get_host_and_port(NEW.url), reverse_host(get_host_and_port(NEW.url)), NEW.frecency);

        -- This is temporary.
        UPDATE moz_places SET
          origin_id = (SELECT id FROM moz_origins
                       WHERE prefix = get_prefix(NEW.url) AND
                             host = get_host_and_port(NEW.url) AND
                             rev_host = reverse_host(get_host_and_port(NEW.url)))
        WHERE id = NEW.id;
    END
";

// Note that while we create tombstones manually, we rely on this trigger to
// delete any which might exist when a new record is written to moz_places.
const CREATE_TRIGGER_MOZPLACES_AFTERINSERT_REMOVE_TOMBSTONES: &str = "
    CREATE TEMP TRIGGER moz_places_afterinsert_trigger_tombstone
    AFTER INSERT ON moz_places
    FOR EACH ROW
    BEGIN
        DELETE FROM moz_places_tombstones WHERE guid = NEW.guid;
    END
";

// Triggers which update visit_count and last_visit_date based on historyvisits
// table changes.
const EXCLUDED_VISIT_TYPES: &str = "0, 4, 7, 8, 9"; // stolen from desktop

lazy_static! {
    static ref CREATE_TRIGGER_HISTORYVISITS_AFTERINSERT: String = format!("
        CREATE TEMP TRIGGER moz_historyvisits_afterinsert_trigger
        AFTER INSERT ON moz_historyvisits FOR EACH ROW
        BEGIN
            UPDATE moz_places SET
                visit_count_remote = visit_count_remote + (NEW.visit_type NOT IN ({excluded}) AND NOT(NEW.is_local)),
                visit_count_local =  visit_count_local + (NEW.visit_type NOT IN ({excluded}) AND NEW.is_local),
                last_visit_date_local = MAX(last_visit_date_local,
                                            CASE WHEN NEW.is_local THEN NEW.visit_date ELSE 0 END),
                last_visit_date_remote = MAX(last_visit_date_remote,
                                             CASE WHEN NEW.is_local THEN 0 ELSE NEW.visit_date END)
            WHERE id = NEW.place_id;
        END", excluded = EXCLUDED_VISIT_TYPES);

    static ref CREATE_TRIGGER_HISTORYVISITS_AFTERDELETE: String = format!("
        CREATE TEMP TRIGGER moz_historyvisits_afterdelete_trigger
        AFTER DELETE ON moz_historyvisits FOR EACH ROW
        BEGIN
            UPDATE moz_places SET
                visit_count_local = visit_count_local - (OLD.visit_type NOT IN ({excluded}) AND OLD.is_local),
                visit_count_remote = visit_count_remote - (OLD.visit_type NOT IN ({excluded}) AND NOT(OLD.is_local)),
                last_visit_date_local = IFNULL((SELECT visit_date FROM moz_historyvisits
                                                WHERE place_id = OLD.place_id AND is_local
                                                ORDER BY visit_date DESC LIMIT 1), 0),
                last_visit_date_remote = IFNULL((SELECT visit_date FROM moz_historyvisits
                                                 WHERE place_id = OLD.place_id AND NOT(is_local)
                                                 ORDER BY visit_date DESC LIMIT 1), 0)
            WHERE id = OLD.place_id;
        END", excluded = EXCLUDED_VISIT_TYPES);
}

const CREATE_TRIGGER_BOOKMARKS_FOREIGNCOUNT_AFTERDELETE: &str = "
    CREATE TEMP TRIGGER moz_bookmarks_foreign_count_afterdelete_trigger
    AFTER DELETE ON moz_bookmarks FOR EACH ROW
    BEGIN
        UPDATE moz_places
        SET foreign_count = foreign_count - 1
        WHERE id = OLD.fk;
    END";

// Note that the desktop versions of the triggers below call a note_sync_change()
// function in some/all cases, which we will probably end up needing when we
// come to sync.
const CREATE_TRIGGER_BOOKMARKS_AFTERINSERT_TRIGGER: &str = "
    CREATE TEMP TRIGGER moz_bookmarks_afterinsert_trigger
    AFTER INSERT ON moz_bookmarks FOR EACH ROW
    BEGIN
        UPDATE moz_places
            SET foreign_count = foreign_count + 1
            WHERE id = NEW.fk;
        DELETE from moz_bookmarks_deleted WHERE guid = NEW.guid;
    END";

const CREATE_TRIGGER_BOOKMARKS_FOREIGNCOUNT_AFTERUPDATE_TRIGGER: &str = "
    CREATE TEMP TRIGGER moz_bookmarks_foreign_count_afterupdate_trigger
    AFTER UPDATE OF fk, syncChangeCounter ON moz_bookmarks FOR EACH ROW
    BEGIN
        UPDATE moz_places
            SET foreign_count = foreign_count + 1
            WHERE OLD.fk <> NEW.fk AND id = NEW.fk;
        UPDATE moz_places
            SET foreign_count = foreign_count - 1
            WHERE OLD.fk <> NEW.fk AND id = OLD.fk;
    END";

// Unlike history, we manage bookmark tombstones via triggers. We do this
// because we rely on foreign-keys to auto-remove children of a deleted folder.
const CREATE_TRIGGER_BOOKMARKS_CREATE_TOMBSTONE: &str = "
    CREATE TEMP TRIGGER moz_create_bookmarks_deleted_trigger
    AFTER DELETE ON moz_bookmarks
    FOR EACH ROW WHEN OLD.syncStatus = 2 -- SyncStatus::Normal
    BEGIN
        INSERT into moz_bookmarks_deleted VALUES (OLD.guid, now());
    END";

// Updating the guid is only allowed by Sync, and it will use a connection
// without some of these triggers - so for now we prevent changing the guid
// of an existing item.
const CREATE_TRIGGER_BOOKMARKS_REMOVE_TOMBSTONE_UPDATE: &str = "
    CREATE TEMP TRIGGER moz_remove_bookmarks_deleted_update_trigger
    AFTER UPDATE ON moz_bookmarks
    FOR EACH ROW WHEN OLD.guid != NEW.guid
    BEGIN
        SELECT RAISE(FAIL, 'guids are immutable');
    END";

// XXX - TODO - lots of desktop temp tables - but it's not clear they make sense here yet?

// XXX - TODO - lots of favicon related tables - but it's not clear they make sense here yet?

// This table holds key-value metadata for Places and its consumers. Sync stores
// the sync IDs for the bookmarks and history collections in this table, and the
// last sync time for history.
const CREATE_TABLE_META_SQL: &str = "CREATE TABLE moz_meta (
        key TEXT PRIMARY KEY,
        value NOT NULL
    ) WITHOUT ROWID";

// See https://searchfox.org/mozilla-central/source/toolkit/components/places/nsPlacesIndexes.h
const CREATE_IDX_MOZ_PLACES_URL_HASH: &str = "CREATE INDEX url_hashindex ON moz_places(url_hash)";

// const CREATE_IDX_MOZ_PLACES_REVHOST: &str = "CREATE INDEX hostindex ON moz_places(rev_host)";

const CREATE_IDX_MOZ_PLACES_VISITCOUNT_LOCAL: &str =
    "CREATE INDEX visitcountlocal ON moz_places(visit_count_local)";
const CREATE_IDX_MOZ_PLACES_VISITCOUNT_REMOTE: &str =
    "CREATE INDEX visitcountremote ON moz_places(visit_count_remote)";

const CREATE_IDX_MOZ_PLACES_FRECENCY: &str = "CREATE INDEX frecencyindex ON moz_places(frecency)";

const CREATE_IDX_MOZ_PLACES_LASTVISITDATE_LOCAL: &str =
    "CREATE INDEX lastvisitdatelocalindex ON moz_places(last_visit_date_local)";
const CREATE_IDX_MOZ_PLACES_LASTVISITDATE_REMOTE: &str =
    "CREATE INDEX lastvisitdateremoteindex ON moz_places(last_visit_date_remote)";

const CREATE_IDX_MOZ_PLACES_GUID: &str = "CREATE UNIQUE INDEX guid_uniqueindex ON moz_places(guid)";

const CREATE_IDX_MOZ_PLACES_ORIGIN_ID: &str = "CREATE INDEX originidindex ON moz_places(origin_id)";

const CREATE_IDX_MOZ_HISTORYVISITS_PLACEDATE: &str =
    "CREATE INDEX placedateindex ON moz_historyvisits(place_id, visit_date)";
const CREATE_IDX_MOZ_HISTORYVISITS_FROMVISIT: &str =
    "CREATE INDEX fromindex ON moz_historyvisits(from_visit)";

const CREATE_IDX_MOZ_HISTORYVISITS_VISITDATE: &str =
    "CREATE INDEX dateindex ON moz_historyvisits(visit_date)";

const CREATE_IDX_MOZ_HISTORYVISITS_ISLOCAL: &str =
    "CREATE INDEX islocalindex ON moz_historyvisits(is_local)";

// const CREATE_IDX_MOZ_BOOKMARKS_PLACETYPE: &str = "CREATE INDEX itemindex ON moz_bookmarks(fk, type)";
// const CREATE_IDX_MOZ_BOOKMARKS_PARENTPOSITION: &str = "CREATE INDEX parentindex ON moz_bookmarks(parent, position)";
const CREATE_IDX_MOZ_BOOKMARKS_PLACELASTMODIFIED: &str =
    "CREATE INDEX itemlastmodifiedindex ON moz_bookmarks(fk, lastModified)";
// const CREATE_IDX_MOZ_BOOKMARKS_DATEADDED: &str = "CREATE INDEX dateaddedindex ON moz_bookmarks(dateAdded)";
// const CREATE_IDX_MOZ_BOOKMARKS_GUID: &str = "CREATE UNIQUE INDEX guid_uniqueindex ON moz_bookmarks(guid)";

// Keys in the moz_meta table.
// pub(crate) static MOZ_META_KEY_ORIGIN_FRECENCY_COUNT: &'static str = "origin_frecency_count";
// pub(crate) static MOZ_META_KEY_ORIGIN_FRECENCY_SUM: &'static str = "origin_frecency_sum";
// pub(crate) static MOZ_META_KEY_ORIGIN_FRECENCY_SUM_OF_SQUARES: &'static str = "origin_frecency_sum_of_squares";

pub fn init(db: &PlacesDb) -> Result<()> {
    let user_version = db.query_one::<i64>("PRAGMA user_version")?;
    if user_version == 0 {
        create(db)?;
    } else if user_version != VERSION {
        if user_version < VERSION {
            upgrade(db, user_version)?;
        } else {
            log::warn!(
                "Loaded future schema version {} (we only understand version {}). \
                 Optimisitically ",
                user_version,
                VERSION
            )
        }
    }
    log::debug!("Creating temp tables and triggers");
    db.execute_all(&[
        CREATE_TRIGGER_AFTER_INSERT_ON_PLACES,
        &CREATE_TRIGGER_HISTORYVISITS_AFTERINSERT,
        &CREATE_TRIGGER_HISTORYVISITS_AFTERDELETE,
        CREATE_TRIGGER_MOZPLACES_AFTERINSERT_REMOVE_TOMBSTONES,
        CREATE_TRIGGER_BOOKMARKS_AFTERINSERT_TRIGGER,
        CREATE_TRIGGER_BOOKMARKS_FOREIGNCOUNT_AFTERDELETE,
        CREATE_TRIGGER_BOOKMARKS_FOREIGNCOUNT_AFTERUPDATE_TRIGGER,
        CREATE_TRIGGER_BOOKMARKS_CREATE_TOMBSTONE,
        CREATE_TRIGGER_BOOKMARKS_REMOVE_TOMBSTONE_UPDATE,
    ])?;
    Ok(())
}

// https://github.com/mozilla-mobile/firefox-ios/blob/master/Storage/SQL/LoginsSchema.swift#L100
fn upgrade(_db: &PlacesDb, from: i64) -> Result<()> {
    log::debug!("Upgrading schema from {} to {}", from, VERSION);
    if from == VERSION {
        return Ok(());
    }
    // FIXME https://github.com/mozilla/application-services/issues/438
    // NB: PlacesConnection.kt checks for this error message verbatim as a workaround.
    panic!("sorry, no upgrades yet - delete your db!");
}

pub fn create(db: &PlacesDb) -> Result<()> {
    log::debug!("Creating schema");
    db.execute_all(&[
        CREATE_TABLE_PLACES_SQL,
        CREATE_TABLE_PLACES_TOMBSTONES_SQL,
        CREATE_TABLE_HISTORYVISITS_SQL,
        CREATE_TABLE_INPUTHISTORY_SQL,
        CREATE_TABLE_BOOKMARKS_SQL,
        CREATE_TABLE_BOOKMARKS_DELETED_SQL,
        CREATE_TABLE_ORIGINS_SQL,
        CREATE_TABLE_META_SQL,
        CREATE_IDX_MOZ_PLACES_URL_HASH,
        CREATE_IDX_MOZ_PLACES_VISITCOUNT_LOCAL,
        CREATE_IDX_MOZ_PLACES_VISITCOUNT_REMOTE,
        CREATE_IDX_MOZ_PLACES_FRECENCY,
        CREATE_IDX_MOZ_PLACES_LASTVISITDATE_LOCAL,
        CREATE_IDX_MOZ_PLACES_LASTVISITDATE_REMOTE,
        CREATE_IDX_MOZ_PLACES_GUID,
        CREATE_IDX_MOZ_PLACES_ORIGIN_ID,
        CREATE_IDX_MOZ_HISTORYVISITS_PLACEDATE,
        CREATE_IDX_MOZ_HISTORYVISITS_FROMVISIT,
        CREATE_IDX_MOZ_HISTORYVISITS_VISITDATE,
        CREATE_IDX_MOZ_HISTORYVISITS_ISLOCAL,
        CREATE_IDX_MOZ_BOOKMARKS_PLACELASTMODIFIED,
    ])?;
    create_bookmark_roots(&db.conn())?;
    db.execute(
        &format!("PRAGMA user_version = {version}", version = VERSION),
        NO_PARAMS,
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::PlacesDb;
    use crate::types::{SyncGuid, SyncStatus};
    use url::Url;

    fn has_tombstone(conn: &PlacesDb, guid: &SyncGuid) -> bool {
        let count: Result<Option<u32>> = conn.try_query_row(
            "SELECT COUNT(*) from moz_places_tombstones
                     WHERE guid = :guid",
            &[(":guid", guid)],
            |row| Ok(row.get_checked::<_, u32>(0)?),
            true,
        );
        count.unwrap().unwrap() == 1
    }

    #[test]
    fn test_places_no_tombstone() {
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let guid = SyncGuid::new();

        conn.execute_named_cached(
            "INSERT INTO moz_places (guid, url, url_hash) VALUES (:guid, :url, hash(:url))",
            &[
                (":guid", &guid),
                (
                    ":url",
                    &Url::parse("http://example.com")
                        .expect("valid url")
                        .into_string(),
                ),
            ],
        )
        .expect("should work");

        let place_id = conn.last_insert_rowid();
        conn.execute_named_cached(
            "DELETE FROM moz_places WHERE id = :id",
            &[(":id", &place_id)],
        )
        .expect("should work");

        // should not have a tombstone.
        assert!(!has_tombstone(&conn, &guid));
    }

    #[test]
    fn test_places_tombstone_removal() {
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let guid = SyncGuid::new();

        conn.execute_named_cached(
            "INSERT INTO moz_places_tombstones VALUES (:guid)",
            &[(":guid", &guid)],
        )
        .expect("should work");

        // insert into moz_places - the tombstone should be removed.
        conn.execute_named_cached(
            "INSERT INTO moz_places (guid, url, url_hash, sync_status)
             VALUES (:guid, :url, hash(:url), :sync_status)",
            &[
                (":guid", &guid),
                (
                    ":url",
                    &Url::parse("http://example.com")
                        .expect("valid url")
                        .into_string(),
                ),
                (":sync_status", &SyncStatus::Normal),
            ],
        )
        .expect("should work");
        assert!(!has_tombstone(&conn, &guid));
    }

    #[test]
    fn test_bookmark_check_constraints() {
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");

        // Invalid value for "type"
        let e = conn
            .execute_cached(
                "INSERT INTO moz_bookmarks
                (fk, type, parent, position, dateAdded, lastModified, guid)
            VALUES
                (NULL, 4, 0, 0, 1, 1, 'fake_guid___')",
                NO_PARAMS,
            )
            .expect_err("should fail");
        // it's a shame the message doesn't indicate what check failed :(
        assert_eq!(e.to_string(), "CHECK constraint failed: moz_bookmarks");

        // type==BOOKMARK but null fk
        let e = conn
            .execute_cached(
                "INSERT INTO moz_bookmarks
                    (fk, type, parent, position, dateAdded, lastModified, guid)
                 VALUES
                    (NULL, 1, 0, 0, 1, 1, 'fake_guid___')",
                NO_PARAMS,
            )
            .expect_err("should fail");
        assert_eq!(e.to_string(), "CHECK constraint failed: moz_bookmarks");

        // type!=BOOKMARK and non-null fk
        let e = conn
            .execute_cached(
                "INSERT INTO moz_bookmarks
                    (fk, type, parent, position, dateAdded, lastModified, guid)
                 VALUES
                    (1, 2, 0, 0, 1, 1, 'fake_guid___')",
                NO_PARAMS,
            )
            .expect_err("should fail");
        assert_eq!(e.to_string(), "CHECK constraint failed: moz_bookmarks");

        // null parent for item other than the root
        let e = conn
            .execute_cached(
                "INSERT INTO moz_bookmarks
                    (fk, type, parent, position, dateAdded, lastModified, guid)
                 VALUES
                    (NULL, 2, NULL, 0, 1, 1, 'fake_guid___')",
                NO_PARAMS,
            )
            .expect_err("should fail");
        assert_eq!(e.to_string(), "CHECK constraint failed: moz_bookmarks");

        // Invalid length guid
        let e = conn
            .execute_cached(
                "INSERT INTO moz_bookmarks
                    (fk, type, parent, position, dateAdded, lastModified, guid)
                 VALUES
                    (NULL, 2, 0, 0, 1, 1, 'fake_guid')",
                NO_PARAMS,
            )
            .expect_err("should fail");
        assert_eq!(e.to_string(), "CHECK constraint failed: moz_bookmarks");

        // negative position
        let e = conn
            .execute_cached(
                "INSERT INTO moz_bookmarks
                    (fk, type, parent, position, dateAdded, lastModified, guid)
                 VALUES
                    (NULL, 2, 0, -1, 1, 1, 'fake_guid___')",
                NO_PARAMS,
            )
            .expect_err("should fail");
        assert_eq!(e.to_string(), "CHECK constraint failed: moz_bookmarks");
    }

    fn select_simple_int(conn: &PlacesDb, stmt: &str) -> u32 {
        let count: Result<Option<u32>> =
            conn.try_query_row(stmt, &[], |row| Ok(row.get_checked::<_, u32>(0)?), false);
        count.unwrap().unwrap()
    }

    fn get_foreign_count(conn: &PlacesDb, guid: &SyncGuid) -> u32 {
        let count: Result<Option<u32>> = conn.try_query_row(
            "SELECT foreign_count from moz_places
                     WHERE guid = :guid",
            &[(":guid", guid)],
            |row| Ok(row.get_checked::<_, u32>(0)?),
            true,
        );
        count.unwrap().unwrap()
    }

    #[test]
    fn test_bookmark_foreign_count_triggers() {
        // create the place.
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");
        let guid1 = SyncGuid::new();
        let url1 = Url::parse("http://example.com")
            .expect("valid url")
            .into_string();
        let guid2 = SyncGuid::new();
        let url2 = Url::parse("http://example2.com")
            .expect("valid url")
            .into_string();

        conn.execute_named_cached(
            "INSERT INTO moz_places (guid, url, url_hash) VALUES (:guid, :url, hash(:url))",
            &[(":guid", &guid1), (":url", &url1)],
        )
        .expect("should work");
        let place_id1 = conn.last_insert_rowid();

        conn.execute_named_cached(
            "INSERT INTO moz_places (guid, url, url_hash) VALUES (:guid, :url, hash(:url))",
            &[(":guid", &guid2), (":url", &url2)],
        )
        .expect("should work");
        let place_id2 = conn.last_insert_rowid();

        assert_eq!(get_foreign_count(&conn, &guid1), 0);
        assert_eq!(get_foreign_count(&conn, &guid2), 0);

        // create a bookmark pointing at it.
        conn.execute_named_cached(
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
        conn.execute_named_cached(
            "UPDATE moz_bookmarks SET fk = :new_place WHERE guid = 'fake_guid___';",
            &[(":new_place", &place_id2)],
        )
        .expect("should work");
        assert_eq!(get_foreign_count(&conn, &guid1), 0);
        assert_eq!(get_foreign_count(&conn, &guid2), 1);

        conn.execute(
            "DELETE FROM moz_bookmarks WHERE guid = 'fake_guid___';",
            NO_PARAMS,
        )
        .expect("should work");
        assert_eq!(get_foreign_count(&conn, &guid1), 0);
        assert_eq!(get_foreign_count(&conn, &guid2), 0);
    }

    #[test]
    fn test_bookmark_delete_restrict() {
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");
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
        conn.execute(
            "DELETE FROM moz_places WHERE guid = 'place_guid__';",
            NO_PARAMS,
        )
        .expect_err("should fail");

        // delete the bookmark.
        conn.execute(
            "DELETE FROM moz_bookmarks WHERE guid = 'fake_guid___';",
            NO_PARAMS,
        )
        .expect("should be able to delete the bookmark");

        // now we should be able to delete the place.
        conn.execute(
            "DELETE FROM moz_places WHERE guid = 'place_guid__';",
            NO_PARAMS,
        )
        .expect("should now be able to delete the place");
    }

    #[test]
    fn test_bookmark_auto_deletes() {
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");

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
        conn.execute(
            "DELETE FROM moz_bookmarks WHERE guid = 'folder_guid_';",
            NO_PARAMS,
        )
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

        // Place should remain.
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
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");
        conn.execute(
            &format!(
                "INSERT INTO moz_bookmarks
                        (syncStatus, type, parent, position, dateAdded, lastModified, guid)
                     VALUES
                        ({}, 3, 1, 0, 1, 1, 'bookmarkguid')",
                SyncStatus::Normal as u8
            ),
            NO_PARAMS,
        )
        .expect("should insert regular bookmark folder");
        conn.execute(
            "DELETE FROM moz_bookmarks WHERE guid = 'bookmarkguid'",
            NO_PARAMS,
        )
        .expect("should delete");
        // should have a tombstone.
        assert_eq!(
            select_simple_int(&conn, "SELECT COUNT(*) from moz_bookmarks_deleted"),
            1
        );
        conn.execute("DELETE from moz_bookmarks_deleted", NO_PARAMS)
            .expect("should delete");
        conn.execute(
            &format!(
                "INSERT INTO moz_bookmarks
                        (syncStatus, type, parent, position, dateAdded, lastModified, guid)
                     VALUES
                        ({}, 3, 1, 0, 1, 1, 'bookmarkguid')",
                SyncStatus::New as u8
            ),
            NO_PARAMS,
        )
        .expect("should insert regular bookmark folder");;
        conn.execute(
            "DELETE FROM moz_bookmarks WHERE guid = 'bookmarkguid'",
            NO_PARAMS,
        )
        .expect("should delete");
        // should not have a tombstone as syncStatus is new.
        assert_eq!(
            select_simple_int(&conn, "SELECT COUNT(*) from moz_bookmarks_deleted"),
            0
        );
    }

    #[test]
    fn test_bookmark_tombstone_auto_deletes() {
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");
        conn.execute(
            "INSERT into moz_bookmarks_deleted VALUES ('bookmarkguid', 1)",
            NO_PARAMS,
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
            NO_PARAMS,
        )
        .expect("should insert regular bookmark folder");;
        // tombstone should have vanished.
        assert_eq!(
            select_simple_int(&conn, "SELECT COUNT(*) from moz_bookmarks_deleted"),
            0
        );
    }

    #[test]
    fn test_bookmark_tombstone_auto_deletes_on_update() {
        let conn = PlacesDb::open_in_memory(None).expect("no memory db");

        // check updates do the right thing.
        conn.execute(
            "INSERT into moz_bookmarks_deleted VALUES ('bookmarkguid', 1)",
            NO_PARAMS,
        )
        .expect("should insert tombstone");

        // create a bookmark with a different guid.
        conn.execute(
            "INSERT INTO moz_bookmarks
                        (type, parent, position, dateAdded, lastModified, guid)
                     VALUES
                        (3, 1, 0, 1, 1, 'fake_guid___')",
            NO_PARAMS,
        )
        .expect("should insert regular bookmark folder");;
        // tombstone should remain.
        assert_eq!(
            select_simple_int(&conn, "SELECT COUNT(*) from moz_bookmarks_deleted"),
            1
        );
        // update guid - should fail as we have a trigger with RAISEs.
        conn.execute(
            "UPDATE moz_bookmarks SET guid = 'bookmarkguid'
             WHERE guid = 'fake_guid___'",
            NO_PARAMS,
        )
        .expect_err("changing the guid should fail");
    }
}

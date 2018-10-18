/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// XXXXXX - This has been cloned from logins-sql/src/schema.rs, on Thom's
// wip-sync-sql-store branch.
// We should work out how to turn this into something that can use a shared
// db.rs.

use db::PlacesDb;
use sql_support::ConnExt;

use error::*;

const VERSION: i64 = 1;

const CREATE_TABLE_PLACES_SQL: &str =
    "CREATE TABLE IF NOT EXISTS moz_places (
        id INTEGER PRIMARY KEY,
        url LONGVARCHAR,
        title LONGVARCHAR,
        -- note - desktop has rev_host here - that's now in moz_origin.
        visit_count_local INTEGER DEFAULT 0,
        visit_count_remote INTEGER DEFAULT 0,
        hidden INTEGER DEFAULT 0 NOT NULL,
        typed INTEGER DEFAULT 0 NOT NULL, -- XXX - is 'typed' ok? Note also we want this as a *count*, not a bool.
        frecency INTEGER DEFAULT -1 NOT NULL,
        -- XXX - splitting last visit into local and remote correct?
        last_visit_date_local INTEGER,
        last_visit_date_remote INTEGER,
        guid TEXT UNIQUE,
        foreign_count INTEGER DEFAULT 0 NOT NULL,
        url_hash INTEGER DEFAULT 0 NOT NULL,
        description TEXT, -- XXXX - title above?
        preview_image_url TEXT,
        origin_id INTEGER, -- NOT NULL XXXX - not clear if there should always be a moz_origin

        FOREIGN KEY(origin_id) REFERENCES moz_origins(id) ON DELETE CASCADE
    )";


const CREATE_TABLE_HISTORYVISITS_SQL: &str =
    "CREATE TABLE moz_historyvisits (
        id INTEGER PRIMARY KEY,
        is_local INTEGER NOT NULL, -- XXX - not in desktop - will always be true for visits added locally, always false visits added by sync.
        from_visit INTEGER, -- XXX - self-reference?
        place_id INTEGER NOT NULL,
        visit_date INTEGER,
        visit_type INTEGER,
        -- session INTEGER, -- XXX - what is 'session'? Appears unused.

        FOREIGN KEY(place_id) REFERENCES moz_places(id) ON DELETE CASCADE,
        FOREIGN KEY(from_visit) REFERENCES moz_historyvisits(id)
    )";

const CREATE_TABLE_INPUTHISTORY_SQL: &str =
    "CREATE TABLE moz_inputhistory (
        place_id INTEGER NOT NULL,
        input LONGVARCHAR NOT NULL,
        use_count INTEGER,

        PRIMARY KEY (place_id, input),
        FOREIGN KEY(place_id) REFERENCES moz_places(id) ON DELETE CASCADE
    )";

// XXX - TODO - moz_annos
// XXX - TODO - moz_anno_attributes
// XXX - TODO - moz_items_annos
// XXX - TODO - moz_bookmarks
// XXX - TODO - moz_bookmarks_deleted

// TODO: This isn't the complete `moz_bookmarks` definition, just enough to
// test autocomplete.
const CREATE_TABLE_BOOKMARKS_SQL: &str =
    "CREATE TABLE moz_bookmarks (
        id INTEGER PRIMARY KEY,
        fk INTEGER,
        title TEXT,
        lastModified INTEGER NOT NULL DEFAULT 0,

        FOREIGN KEY(fk) REFERENCES moz_places(id) ON DELETE RESTRICT
    )";


// Note: desktop has/had a 'keywords' table, but we intentionally do not.

const CREATE_TABLE_ORIGINS_SQL: &str =
    "CREATE TABLE moz_origins (
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

// XXX - TODO - lots of desktop temp tables - but it's not clear they make sense here yet?

// XXX - TODO - lots of favicon related tables - but it's not clear they make sense here yet?

// This table holds key-value metadata for Places and its consumers. Sync stores
// the sync IDs for the bookmarks and history collections in this table, and the
// last sync time for history.
const CREATE_TABLE_META_SQL: &str =
    "CREATE TABLE moz_meta (
        key TEXT PRIMARY KEY,
        value NOT NULL
    ) WITHOUT ROWID";

// See https://searchfox.org/mozilla-central/source/toolkit/components/places/nsPlacesIndexes.h
const CREATE_IDX_MOZ_PLACES_URL_HASH: &str = "CREATE INDEX url_hashindex ON moz_places(url_hash)";

// const CREATE_IDX_MOZ_PLACES_REVHOST: &str = "CREATE INDEX hostindex ON moz_places(rev_host)";

const CREATE_IDX_MOZ_PLACES_VISITCOUNT_LOCAL: &str = "CREATE INDEX visitcountlocal ON moz_places(visit_count_local)";
const CREATE_IDX_MOZ_PLACES_VISITCOUNT_REMOTE: &str = "CREATE INDEX visitcountremote ON moz_places(visit_count_remote)";

const CREATE_IDX_MOZ_PLACES_FRECENCY: &str = "CREATE INDEX frecencyindex ON moz_places(frecency)";

const CREATE_IDX_MOZ_PLACES_LASTVISITDATE_LOCAL: &str = "CREATE INDEX lastvisitdatelocalindex ON moz_places(last_visit_date_local)";
const CREATE_IDX_MOZ_PLACES_LASTVISITDATE_REMOTE: &str = "CREATE INDEX lastvisitdateremoteindex ON moz_places(last_visit_date_remote)";

const CREATE_IDX_MOZ_PLACES_GUID: &str = "CREATE UNIQUE INDEX guid_uniqueindex ON moz_places(guid)";

const CREATE_IDX_MOZ_PLACES_ORIGIN_ID: &str = "CREATE INDEX originidindex ON moz_places(origin_id)";

const CREATE_IDX_MOZ_HISTORYVISITS_PLACEDATE: &str = "CREATE INDEX placedateindex ON moz_historyvisits(place_id, visit_date)";
const CREATE_IDX_MOZ_HISTORYVISITS_FROMVISIT: &str = "CREATE INDEX fromindex ON moz_historyvisits(from_visit)";

const CREATE_IDX_MOZ_HISTORYVISITS_VISITDATE: &str = "CREATE INDEX dateindex ON moz_historyvisits(visit_date)";

const CREATE_IDX_MOZ_HISTORYVISITS_ISLOCAL: &str = "CREATE INDEX islocalindex ON moz_historyvisits(is_local)";


// const CREATE_IDX_MOZ_BOOKMARKS_PLACETYPE: &str = "CREATE INDEX itemindex ON moz_bookmarks(fk, type)";
// const CREATE_IDX_MOZ_BOOKMARKS_PARENTPOSITION: &str = "CREATE INDEX parentindex ON moz_bookmarks(parent, position)";
const CREATE_IDX_MOZ_BOOKMARKS_PLACELASTMODIFIED: &str = "CREATE INDEX itemlastmodifiedindex ON moz_bookmarks(fk, lastModified)";
// const CREATE_IDX_MOZ_BOOKMARKS_DATEADDED: &str = "CREATE INDEX dateaddedindex ON moz_bookmarks(dateAdded)";
// const CREATE_IDX_MOZ_BOOKMARKS_GUID: &str = "CREATE UNIQUE INDEX guid_uniqueindex ON moz_bookmarks(guid)";

// Keys in the moz_meta table.
// pub(crate) static MOZ_META_KEY_ORIGIN_FRECENCY_COUNT: &'static str = "origin_frecency_count";
// pub(crate) static MOZ_META_KEY_ORIGIN_FRECENCY_SUM: &'static str = "origin_frecency_sum";
// pub(crate) static MOZ_META_KEY_ORIGIN_FRECENCY_SUM_OF_SQUARES: &'static str = "origin_frecency_sum_of_squares";


pub fn init(db: &PlacesDb) -> Result<()> {
    let user_version = db.query_one::<i64>("PRAGMA user_version")?;
    if user_version == 0 {
        return create(db);
    }
    if user_version != VERSION {
        if user_version < VERSION {
            upgrade(db, user_version)?;
        } else {
            warn!("Loaded future schema version {} (we only understand version {}). \
                   Optimisitically ",
                  user_version, VERSION)
        }
    }
    Ok(())
}

// https://github.com/mozilla-mobile/firefox-ios/blob/master/Storage/SQL/LoginsSchema.swift#L100
fn upgrade(_db: &PlacesDb, from: i64) -> Result<()> {
    debug!("Upgrading schema from {} to {}", from, VERSION);
    if from == VERSION {
        return Ok(());
    }
    // hrmph - do something here?
    panic!("sorry, no upgrades yet - delete your db!");
}

pub fn create(db: &PlacesDb) -> Result<()> {
    debug!("Creating schema");
    db.execute_all(&[
        CREATE_TABLE_PLACES_SQL,
        CREATE_TABLE_HISTORYVISITS_SQL,
        CREATE_TABLE_INPUTHISTORY_SQL,
        CREATE_TABLE_BOOKMARKS_SQL,
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
        &format!("PRAGMA user_version = {version}",
                 version = VERSION),
    ])?;

    debug!("Creating temp tables and triggers");
    db.execute_all(&[
        CREATE_TRIGGER_AFTER_INSERT_ON_PLACES,
    ])?;

    Ok(())
}

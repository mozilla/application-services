/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::time::Instant;

use crate::error::{info, Result};
use crate::history_sync::engine::LAST_SYNC_META_KEY;
use crate::import::common::{
    attached_database, define_history_migration_functions, select_count, HistoryMigrationResult,
};
use crate::storage::{put_meta, update_all_frecencies_at_once};
use crate::PlacesDb;
use types::Timestamp;
use url::Url;

/// This import is used for iOS users migrating from `browser.db`-based
/// history storage to the new rust-places store.
///
/// The goal of this import is to persist all local browser.db items into places database
///
///
/// ### Basic process
///
/// - Attach the iOS database.
/// - Slurp records into a temp table "iOSHistoryStaging" from iOS database.
///   - This is mostly done for convenience, to punycode the URLs and some performance benefits over
///     using a view or reading things into Rust
/// - Add any entries to moz_places that are needed (in practice, most are
///   needed, users in practice don't have nearly as many bookmarks as history entries)
/// - Use iosHistoryStaging and the browser.db to migrate visits to the places visits table.
/// - Update frecency for new items.
/// - Cleanup (detach iOS database, etc).
pub fn import(
    conn: &PlacesDb,
    path: impl AsRef<std::path::Path>,
    last_sync_timestamp: i64,
) -> Result<HistoryMigrationResult> {
    let url = crate::util::ensure_url_path(path)?;
    do_import(conn, url, last_sync_timestamp)
}

fn do_import(
    conn: &PlacesDb,
    ios_db_file_url: Url,
    last_sync_timestamp: i64,
) -> Result<HistoryMigrationResult> {
    let scope = conn.begin_interrupt_scope()?;
    define_history_migration_functions(conn)?;
    // TODO: for some reason opening the db as read-only in **iOS** causes
    // the migration to fail with an "attempting to write to a read-only database"
    // when the migration is **not** writing to the BrowserDB database.
    // this only happens in the simulator with artifacts built for iOS and not
    // in unit tests.

    // ios_db_file_url.query_pairs_mut().append_pair("mode", "ro");
    let import_start = Instant::now();
    info!("Attaching database {}", ios_db_file_url);
    let auto_detach = attached_database(conn, &ios_db_file_url, "ios")?;
    let tx = conn.begin_transaction()?;
    let num_total = select_count(conn, &COUNT_IOS_HISTORY_VISITS)?;
    info!("The number of visits is: {:?}", num_total);

    info!("Creating and populating staging table");

    tx.execute_batch(&CREATE_TEMP_VISIT_TABLE)?;
    tx.execute_batch(&FILL_VISIT_TABLE)?;
    tx.execute_batch(&CREATE_STAGING_TABLE)?;
    tx.execute_batch(&FILL_STAGING)?;
    scope.err_if_interrupted()?;

    info!("Updating old titles that may be missing, but now are available");
    tx.execute_batch(&UPDATE_PLACES_TITLES)?;
    scope.err_if_interrupted()?;

    info!("Populating missing entries in moz_places");
    tx.execute_batch(&FILL_MOZ_PLACES)?;
    scope.err_if_interrupted()?;

    info!("Inserting the history visits");
    tx.execute_batch(&INSERT_HISTORY_VISITS)?;
    scope.err_if_interrupted()?;

    info!("Insert all new entries into stale frecencies");
    let now = Timestamp::now().as_millis();
    tx.execute(&ADD_TO_STALE_FRECENCIES, &[(":now", &now)])?;
    scope.err_if_interrupted()?;

    // Once the migration is done, we also migrate the sync timestamp if we have one
    // this prevents us from having to do a **full** sync
    put_meta(conn, LAST_SYNC_META_KEY, &last_sync_timestamp)?;

    tx.commit()?;
    info!("Successfully imported history visits!");

    info!("Counting Places history visits");

    let num_succeeded = select_count(conn, &COUNT_PLACES_HISTORY_VISITS)?;
    let num_failed = num_total.saturating_sub(num_succeeded);

    // We now update the frecencies as its own transaction
    // this is desired because we want reader connections to
    // read the migrated data and not have to wait for the
    // frecencies to be up to date
    info!("Updating all frecencies");
    update_all_frecencies_at_once(conn, &scope)?;
    info!("Frecencies updated!");
    auto_detach.execute_now()?;

    Ok(HistoryMigrationResult {
        num_total,
        num_succeeded,
        num_failed,
        total_duration: import_start.elapsed().as_millis() as u64,
    })
}

lazy_static::lazy_static! {
   // Count IOS history visits
   static ref COUNT_IOS_HISTORY_VISITS: &'static str =
       "SELECT COUNT(*) FROM ios.visits v
        LEFT JOIN ios.history h on v.siteID = h.id
        WHERE h.is_deleted = 0"
   ;

   // Create a temporary table for visists
   static ref CREATE_TEMP_VISIT_TABLE: &'static str = "
    CREATE TEMP TABLE IF NOT EXISTS temp.latestVisits(
        id INTEGER PRIMARY KEY,
        siteID INTEGER NOT NULL,
        date REAL NOT NULL,
        type INTEGER NOT NULL,
        is_local TINYINT NOT NULL
    ) WITHOUT ROWID;
   ";

   // Insert into temp visit table
   static ref FILL_VISIT_TABLE: &'static str = "
    INSERT OR IGNORE INTO temp.latestVisits(id, siteID, date, type, is_local)
        SELECT
            id,
            siteID,
            date,
            type,
            is_local
        FROM ios.visits
        ORDER BY date DESC
        LIMIT 10000
   ";

   // We use a staging table purely so that we can normalize URLs (and
   // specifically, punycode them)
   static ref CREATE_STAGING_TABLE: &'static str = "
        CREATE TEMP TABLE IF NOT EXISTS temp.iOSHistoryStaging(
            id INTEGER PRIMARY KEY,
            url TEXT,
            url_hash INTEGER NOT NULL,
            title TEXT
        ) WITHOUT ROWID;";

   static ref FILL_STAGING: &'static str = "
    INSERT OR IGNORE INTO temp.iOSHistoryStaging(id, url, url_hash, title)
        SELECT
            h.id,
            validate_url(h.url),
            hash(validate_url(h.url)),
            sanitize_utf8(h.title)
        FROM temp.latestVisits v
        JOIN ios.history h on v.siteID = h.id
        WHERE h.url IS NOT NULL
        AND h.is_deleted = 0
        "
   ;

    // Unfortunately UPDATE FROM is not available until sqlite 3.33
   // however, iOS does not ship with 3.33 yet as of the time of writing.
   static ref UPDATE_PLACES_TITLES: &'static str =
   "UPDATE main.moz_places
        SET title = IFNULL((SELECT t.title
                            FROM temp.iOSHistoryStaging t
                            WHERE t.url_hash = main.moz_places.url_hash AND t.url = main.moz_places.url), title)"
    ;

   // Insert any missing entries into moz_places that we'll need for this.
   static ref FILL_MOZ_PLACES: &'static str =
   "INSERT OR IGNORE INTO main.moz_places(guid, url, url_hash, title, frecency, sync_change_counter)
        SELECT
            IFNULL(
                (SELECT p.guid FROM main.moz_places p WHERE p.url_hash = t.url_hash AND p.url = t.url),
                generate_guid()
            ),
            t.url,
            t.url_hash,
            t.title,
            -1,
            1
        FROM temp.iOSHistoryStaging t
   "
   ;

   // Insert history visits
   static ref INSERT_HISTORY_VISITS: &'static str =
   "INSERT OR IGNORE INTO main.moz_historyvisits(from_visit, place_id, visit_date, visit_type, is_local)
        SELECT
            NULL, -- iOS does not store enough information to rebuild redirect chains.
            (SELECT p.id FROM main.moz_places p WHERE p.url_hash = t.url_hash AND p.url = t.url),
            sanitize_float_timestamp(v.date),
            v.type, -- iOS stores visit types that map 1:1 to ours.
            v.is_local
        FROM temp.latestVisits v
        JOIN temp.iOSHistoryStaging t on v.siteID = t.id
    "
   ;


   // Count places history visits
   static ref COUNT_PLACES_HISTORY_VISITS: &'static str =
       "SELECT COUNT(*) FROM main.moz_historyvisits"
   ;

   // Adds newly modified places entries into the stale frecencies table
   static ref ADD_TO_STALE_FRECENCIES: &'static str =
   "INSERT OR IGNORE INTO main.moz_places_stale_frecencies(place_id, stale_at)
    SELECT
        p.id,
        :now
    FROM main.moz_places p
    WHERE p.frecency = -1"
    ;
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::time::Instant;

use crate::api::places_api::PlacesApi;
use crate::bookmark_sync::engine::update_frecencies;
use crate::error::Result;
use crate::import::common::{
    attached_database, define_history_migration_functions, select_count, HistoryMigrationResult,
};
use url::Url;

pub fn import(
    places_api: &PlacesApi,
    path: impl AsRef<std::path::Path>,
) -> Result<HistoryMigrationResult> {
    let url = crate::util::ensure_url_path(path)?;
    do_import(places_api, url)
}

fn do_import(places_api: &PlacesApi, ios_db_file_url: Url) -> Result<HistoryMigrationResult> {
    let conn_mutex = places_api.get_sync_connection()?;
    let conn = conn_mutex.lock();
    let scope = conn.begin_interrupt_scope()?;
    define_history_migration_functions(&conn)?;

    let import_start = Instant::now();
    log::trace!("Attaching database {}", ios_db_file_url);
    let auto_detach = attached_database(&conn, &ios_db_file_url, "ios")?;
    let tx = conn.begin_transaction()?;
    let num_total = select_count(&conn, &COUNT_IOS_HISTORY_VISITS);
    log::info!("The number of visits is: {:?}", num_total);

    log::debug!("Creating and populating staging table");
    conn.execute_batch(&CREATE_STAGING_TABLE)?;
    conn.execute_batch(&FILL_STAGING)?;
    scope.err_if_interrupted()?;

    log::debug!("Populating missing entries in moz_places");
    conn.execute_batch(&FILL_MOZ_PLACES)?;
    scope.err_if_interrupted()?;

    log::debug!("Inserting the history visits");
    conn.execute_batch(&INSERT_HISTORY_VISITS)?;
    scope.err_if_interrupted()?;

    tx.commit()?;
    // Note: update_frecencies manages its own transaction, which is fine,
    // since nothing that bad will happen if it is aborted.
    log::debug!("Updating frecencies");
    update_frecencies(&conn, &scope)?;

    log::info!("Successfully imported history visits!");

    log::debug!("Counting Places history visits");
    let num_succeeded = select_count(&conn, &COUNT_PLACES_HISTORY_VISITS);
    let num_failed = num_total - num_succeeded;

    auto_detach.execute_now()?;

    let metrics = HistoryMigrationResult {
        num_total,
        num_succeeded,
        num_failed,
        total_duration: import_start.elapsed().as_millis(),
    };

    Ok(metrics)
}

lazy_static::lazy_static! {
   // Count IOS history visits
   static ref COUNT_IOS_HISTORY_VISITS: &'static str =
       "SELECT COUNT(*) FROM ios.visits"
   ;

   // We use a staging table purely so that we can normalize URLs (and
   // specifically, punycode them)
   static ref CREATE_STAGING_TABLE: &'static str = "
        CREATE TEMP TABLE temp.iOSHistoryStaging(
            id INTEGER PRIMARY KEY,
            guid TEXT NOT NULL UNIQUE,
            url TEXT,
            url_hash INTEGER NOT NULL,
            title TEXT
        ) WITHOUT ROWID;";

   static ref FILL_STAGING: &'static str = "
    INSERT OR IGNORE INTO temp.iOSHistoryStaging(id, guid, url, url_hash, title)
        SELECT
            h.id,
            sanitize_utf8(guid), -- The places record in our DB may be different, but we
                                    -- need this to join to Fennec's visits table.
            validate_url(h.url),
            hash(validate_url(h.url)),
            sanitize_utf8(h.title)
        FROM ios.history h
        WHERE url IS NOT NULL"
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
        FROM temp.iOSHistoryStaging t"
   ;

   // Insert history visits
   static ref INSERT_HISTORY_VISITS: &'static str =
   "INSERT OR IGNORE INTO main.moz_historyvisits(from_visit, place_id, visit_date, visit_type, is_local)
        SELECT
            NULL, -- Fenec does not store enough information to rebuild redirect chains.
            (SELECT p.id FROM main.moz_places p WHERE p.url_hash = t.url_hash AND p.url = t.url),
            sanitize_timestamp(v.date),
            v.type, -- Fennec stores visit types maps 1:1 to ours.
            v.is_local
        FROM ios.visits v
        LEFT JOIN temp.iOSHistoryStaging t on v.siteID = t.id"
   ;


   // Count places history visits
   static ref COUNT_PLACES_HISTORY_VISITS: &'static str =
       "SELECT COUNT(*) FROM main.moz_historyvisits"
   ;
}

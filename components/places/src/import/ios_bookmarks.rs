/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::api::places_api::{PlacesApi, SyncConn};
use crate::bookmark_sync::{
    store::{BookmarksStore, Merger},
    SyncedBookmarkKind,
};
use crate::error::*;
use crate::storage::URL_LENGTH_MAX;
use crate::types::SyncStatus;
use sql_support::ConnExt;
use url::Url;

pub fn import_ios_bookmarks(places_api: &PlacesApi, mut ios_db_file_url: Url) -> Result<()> {
    let conn = places_api.open_sync_connection()?;

    let scope = conn.begin_interrupt_scope();
    ios_db_file_url.query_pairs_mut().append_pair("mode", "ro");
    log::trace!("Attaching database {}", ios_db_file_url);

    let auto_detach = attached_database(&conn, &ios_db_file_url)?;
    scope.err_if_interrupted()?;
    // Check if there's any point to importing. If they only have 5 items, they're probably
    // a sync user.
    if conn.query_one::<i64>("SELECT count(*) FROM ios.bookmarksLocal")? == 5 {
        // We could check here that all 5 records are the roots, but the only thing
        // we could really do if they aren't is report corruption, so we just
        // pretend that the import went fine.
        log::debug!(
            "Only 5 items (roots) in bookmarksLocal, nothing to import \
             (possibly a sync user)"
        );

        return Ok(());
    }

    // Note: `Drop` is executed in FIFO order, so this will happen after
    // the transaction's drop, which is what we want.
    let _clear_mirror_on_drop = ExecuteOnDrop {
        conn: &conn,
        sql: &WIPE_MIRROR,
    };

    // Clear the mirror now, since we're about to fill it with data from the ios
    // connection.
    log::debug!("Clearing mirror to prepare for import");
    conn.execute_batch(&WIPE_MIRROR)?;

    scope.err_if_interrupted()?;

    log::debug!("Importing from iOS to mirror");
    conn.execute_batch(&IMPORT)?;

    // detach now. (Otherwise we need to specify `main.blah` for a bunch of
    // queries in bookmark sync in cases where it's ambiguous)
    drop(auto_detach);

    scope.err_if_interrupted()?;
    let store = BookmarksStore::new(&conn, &scope);
    let mut merger = Merger::new(&store, Default::default());
    log::debug!("Merging with local records");
    merger.merge()?;
    scope.err_if_interrupted()?;

    // Update last modification time, sync status, etc
    {
        log::debug!("Reattaching ios database to update modification times and flags");
        let _auto_detach = attached_database(&conn, &ios_db_file_url)?;
        conn.execute_batch(&FIXUP)?;
    }

    log::debug!("Updating frecencies");

    store.update_frecencies()?;

    // XXX we probably need to do something to sync status and such?

    Ok(())
}

const ROOTS: &str =
    "('root________', 'menu________', 'toolbar_____', 'unfiled_____', 'mobile______')";

lazy_static::lazy_static! {
    static ref WIPE_MIRROR: String = format!(
        "DELETE FROM moz_bookmarks_synced
           WHERE guid NOT IN {roots};
         DELETE FROM moz_bookmarks_synced_structure
           WHERE guid NOT IN {roots};",
        roots = ROOTS,
    );

    static ref FIXUP: String = format!(
        // Is there anything else?
        "UPDATE main.moz_bookmarks SET
           syncStatus = {unknown},
           syncChangeCounter = 0,
           lastModified = IFNULL((SELECT ib.local_modified FROM ios.bookmarksLocal ib
                                  WHERE ib.guid = main.moz_bookmarks.guid),
                                 now());",
        unknown = SyncStatus::Unknown as u8
    );

    static ref IMPORT: String = format!(
        "-- Insert any missing entries into moz_places
        INSERT OR IGNORE INTO main.moz_places(guid, url, url_hash, frecency)
        SELECT IFNULL((SELECT p.guid FROM main.moz_places p
                       WHERE p.url_hash = hash(b.bmkUri) AND p.url = b.bmkUri),
                      generate_guid()),
               b.bmkUri,
               hash(b.bmkUri),
               CASE substr(b.bmkUri, 1, 6) WHEN 'place:' THEN 0 ELSE -1 END
        FROM ios.bookmarksLocal b
        WHERE b.bmkUri IS NOT NULL
          -- AND b.type = 1
          AND length(b.bmkUri) < {url_length_max}
        ;
        -- Insert items into moz_bookmarks_synced
        REPLACE INTO main.moz_bookmarks_synced(
            guid,
            parentGuid,
            serverModified,
            needsMerge,
            validity,
            isDeleted,
            kind,
            dateAdded,
            title,
            placeId,
            keyword,
            description,
            loadInSidebar,
            smartBookmarkName,
            feedURL,
            siteURL
        )
        SELECT
            b.guid,
            s.parent,
            /* serverModified */ now(),
            /* needsMerge */ 1,
            1, -- VALIDITY_VALID, is this sane??
            b.is_deleted,
            /* map ios bookmark kind to ours.
                https://github.com/mozilla-mobile/firefox-ios/blob/bd08cd4d/Storage/Bookmarks/Bookmarks.swift#L192
                ios:
                    bookmark = 1
                    folder = 2
                    separator = 3
                    dynamicContainer = 4 (we treat this like livemarks)
                    livemark = 5
                    query = 6
            */
            CASE b.type
                WHEN 1 THEN {bookmark_kind}
                WHEN 2 THEN {folder_kind}
                WHEN 3 THEN {separator_kind}
                WHEN 5 THEN {livemark_kind}
                WHEN 6 THEN {query_kind}
            END,
            b.date_added,
            IFNULL(b.title, ''),
            -- placeId
            CASE WHEN b.bmkUri IS NULL
            THEN NULL
            ELSE (SELECT id FROM main.moz_places p
                  WHERE p.url_hash = hash(b.bmkUri) AND p.url = b.bmkUri)
            END,
            b.keyword,
            b.description,
            /* loadInSidebar */ NULL,
            /* smartBookmarkName */ NULL,
            b.feedUri,
            b.siteUri
        FROM ios.bookmarksLocal b
        JOIN ios.bookmarksLocalStructure s
            ON b.guid = s.child
        WHERE b.type IN (1, 2, 3, 5, 6)
          AND (b.bmkUri IS NULL OR length(b.bmkUri) < {url_length_max})
            -- Is this right?
            AND b.guid NOT IN {roots}
        ;
        -- Insert items into moz_bookmarks_synced_structure
        REPLACE INTO main.moz_bookmarks_synced_structure(guid, parentGuid, position)
             SELECT s.child, s.parent, s.idx FROM ios.bookmarksLocalStructure s
        ;",
        roots = ROOTS,
        bookmark_kind = SyncedBookmarkKind::Bookmark as u8,
        folder_kind = SyncedBookmarkKind::Folder as u8,
        separator_kind = SyncedBookmarkKind::Separator as u8,
        livemark_kind = SyncedBookmarkKind::Livemark as u8,
        query_kind = SyncedBookmarkKind::Query as u8,
        url_length_max = URL_LENGTH_MAX,
    );
}

fn attached_database<'a>(conn: &'a SyncConn<'a>, path: &Url) -> Result<ExecuteOnDrop<'a>> {
    conn.execute_named(
        "ATTACH DATABASE :path AS ios",
        rusqlite::named_params! {
            ":path": path.as_str(),
        },
    )?;
    Ok(ExecuteOnDrop {
        conn,
        sql: "DETACH DATABASE ios;",
    })
}

// We use/abuse the mirror to perform our import, but need to clean it up afterwards.
// This is an RAII helper to do so.
struct ExecuteOnDrop<'a> {
    conn: &'a SyncConn<'a>,
    // Logged on errors, so &'static helps discourage using anything
    // that could have user data.
    sql: &'static str,
}

impl Drop for ExecuteOnDrop<'_> {
    fn drop(&mut self) {
        log::trace!("Executing on drop: {}", self.sql);
        if let Err(e) = self.conn.execute_batch(self.sql) {
            log::error!("Failed to clean up after import! {}", e);
            log::debug!("  Failed query: {}", self.sql);
        }
    }
}

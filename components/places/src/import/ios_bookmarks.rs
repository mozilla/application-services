/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This import is used for iOS sync users migrating from `browser.db`-based
//! bookmark storage to the new rust-places store.
//!
//! It is only used for users who are not connected to sync, as syncing
//! bookmarks will go through a more reliable, robust, and well-tested path, and
//! will migrate things that are unavailable on iOS due to the unfortunate
//! history of iOS bookmark sync (accurate last modified times, for example).
//!
//! As a result, the goals of this import are as follows:
//!
//! 1. Any locally created items must be persisted.
//!
//! 2. Any items from remote machines that are visible to the user must be
//!    persisted. (Note: before writing this, most of us believed that iOS wiped
//!    it's view of remote bookmarks on sync sign-out. Apparently it does not,
//!    and it's unclear if it ever did).
//!
//! ### Unsupported features
//!
//! As such, the following things are explicitly not imported:
//!
//! - Livemarks: We don't support them in our database anyway.
//! - Queries: Not displayed or creatable in iOS UI, and only half-supported in
//!   this database.
//! - Tags: Not displayed or creatable in iOS UI, and only half-supported in
//!   this database.
//!
//! The second two cases are unfortunate, but the only time where it will matter
//! is for users:
//!
//! - Who once used sync, but no longer do.
//! - Who used tags and queries when they used sync.
//! - Who no longer have access to any firefoxes from when they were sync users,
//!   other than this iOS device.
//!
//! For these users, upon signing into sync once again, they will not have any
//! queries or tags.
//!
//! ### Basic process
//!
//! - Attach the iOS database.
//! - Slurp records into a temp table "iosBookmarksStaging" from iOS database.
//! - Fill mirror using iosBookmarksStaging
//! - Fill mirror structure using both iOS database and iosBookmarksStaging
//! - Detach the iOS database
//! - Run dogear merge
//! - Use iosBookmarksStaging to fixup the data that was actually inserted.
//! - Delete mirror and mirror structure
//!

use crate::api::places_api::{PlacesApi, SyncConn};
use crate::bookmark_sync::{
    store::{BookmarksStore, Merger},
    SyncedBookmarkKind,
};
use crate::error::*;
use crate::storage::URL_LENGTH_MAX;
use crate::types::SyncStatus;
use url::Url;

pub fn import_ios_bookmarks(
    places_api: &PlacesApi,
    path: impl AsRef<std::path::Path>,
) -> Result<()> {
    let url = crate::util::ensure_url_path(path)?;
    do_import_ios_bookmarks(places_api, url)
}

fn do_import_ios_bookmarks(places_api: &PlacesApi, ios_db_file_url: Url) -> Result<()> {
    let conn = places_api.open_sync_connection()?;

    let scope = conn.begin_interrupt_scope();

    // Not sure why, but apparently beginning a transaction sometimes
    // fails if we open the DB as read-only. Hopefully we don't
    // unintentionally write to it anywhere...
    // ios_db_file_url.query_pairs_mut().append_pair("mode", "ro");

    conn.execute_batch(CREATE_STAGING_TABLE)?;

    log::trace!("Attaching database {}", ios_db_file_url);
    let auto_detach = attached_database(&conn, &ios_db_file_url)?;

    let tx = conn.begin_transaction()?;
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

    log::debug!("Importing from iOS to staging table");
    conn.execute_batch(&POPULATE_STAGING)?;
    scope.err_if_interrupted()?;

    log::debug!("Populating missing entries in moz_places");
    conn.execute_batch(&FILL_MOZ_PLACES)?;
    scope.err_if_interrupted()?;

    log::debug!("Populating mirror");
    conn.execute_batch(&POPULATE_MIRROR)?;
    scope.err_if_interrupted()?;

    // Ideally we could just do this right after `CREATE_AND_POPULATE_STAGING`,
    // which would mean we could detach the iOS database sooner, but we have
    // constraints on the mirror structure that prevent this (and there's
    // probably nothing bad that can happen in this case anyway).
    log::debug!("Populating mirror structure");
    conn.execute_batch(&POPULATE_MIRROR_STRUCTURE)?;
    scope.err_if_interrupted()?;

    log::debug!("Detaching iOS database");
    drop(auto_detach);
    scope.err_if_interrupted()?;

    let store = BookmarksStore::new(&conn, &scope);
    let mut merger = Merger::new(&store, Default::default());
    // We're already in a transaction.
    merger.set_external_transaction(true);
    log::debug!("Merging with local records");
    merger.merge()?;
    scope.err_if_interrupted()?;

    // Update last modification time, sync status, etc
    log::debug!("Fixing up bookmarks");
    conn.execute_batch(&FIXUP_MOZ_BOOKMARKS)?;
    scope.err_if_interrupted()?;
    log::debug!("Committing...");
    tx.commit()?;

    // Note: update_frecencies manages it's own transaction, which is fine,
    // since nothing that bad will happen if it is aborted.
    log::debug!("Updating frecencies");
    store.update_frecencies()?;
    log::debug!("Committing frecency transaction...");

    // This goes away when the connection closes (which is at the end of the
    // scope), but clear it explicitly in case someone forgets to update this
    // when we get around to
    // https://github.com/mozilla/application-services/issues/952
    conn.execute("DROP TABLE temp.iosBookmarksStaging", rusqlite::NO_PARAMS)?;

    log::info!("Successfully imported bookmarks!");

    // Note: The Mirror is cleaned up by `_clear_mirror_on_drop` automatically.

    Ok(())
}

#[derive(Clone, Copy, PartialEq, PartialOrd, Hash, Debug, Eq, Ord)]
#[repr(u8)]
enum IosBookmarkType {
    // https://github.com/mozilla-mobile/firefox-ios/blob/bd08cd4d/Storage/Bookmarks/Bookmarks.swift#L192
    Bookmark = 1,
    Folder = 2,
    Separator = 3,
    // Not supported
    // DynamicContainer = 4,
    // Livemark = 5,
    // Query = 6,
}

const ROOTS: &str =
    "('root________', 'menu________', 'toolbar_____', 'unfiled_____', 'mobile______')";

lazy_static::lazy_static! {
    static ref WIPE_MIRROR: String = format!(
        // Is omitting the roots right?
        "DELETE FROM main.moz_bookmarks_synced
           WHERE guid NOT IN {roots};
         DELETE FROM main.moz_bookmarks_synced_structure
           WHERE guid NOT IN {roots};",
        roots = ROOTS,
    );
    // We omit:
    // - queries, since they don't show up in the iOS UI,
    // - livemarks, because we'd delete them
    // - dynamicContainers, because nobody knows what the hell they are.
    static ref IOS_VALID_TYPES: String = format!(
        "({bookmark_type}, {folder_type}, {separator_type})",
        bookmark_type = IosBookmarkType::Bookmark as u8,
        folder_type = IosBookmarkType::Folder as u8,
        separator_type = IosBookmarkType::Separator as u8,
    );

    // Insert any missing entries into moz_places that we'll need for this.
    static ref FILL_MOZ_PLACES: String = format!(
        "INSERT OR IGNORE INTO main.moz_places(guid, url, url_hash, frecency)
         SELECT IFNULL((SELECT p.guid FROM main.moz_places p
                        WHERE p.url_hash = hash(b.bmkUri) AND p.url = b.bmkUri),
                       generate_guid()),
                b.bmkUri,
                hash(b.bmkUri),
                CASE substr(b.bmkUri, 1, 6) WHEN 'place:' THEN 0 ELSE -1 END
         FROM temp.iosBookmarksStaging b
         WHERE b.bmkUri IS NOT NULL
           AND b.type = {bookmark_type}
           AND length(b.bmkUri) < {url_length_max}",
        url_length_max = URL_LENGTH_MAX,
        bookmark_type = IosBookmarkType::Bookmark as u8,
    );

    static ref POPULATE_MIRROR: String = format!(
        "REPLACE INTO main.moz_bookmarks_synced(
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
            description
        )
        SELECT
            b.guid,
            b.parentid,
            b.modified,
            1, -- needsMerge
            1, -- VALIDITY_VALID, is this sane??
            0, -- isDeleted
            CASE b.type
                WHEN {ios_bookmark_type} THEN {bookmark_kind}
                WHEN {ios_folder_type} THEN {folder_kind}
                WHEN {ios_separator_type} THEN {separator_kind}
                -- We filter out anything else when inserting into the stage table
            END,
            IFNULL(b.date_added, now()),
            IFNULL(b.title, ''),
            -- placeId
            CASE WHEN b.bmkUri IS NULL
            THEN NULL
            ELSE (SELECT id FROM main.moz_places p
                  WHERE p.url_hash = hash(b.bmkUri) AND p.url = b.bmkUri)
            END,
            b.keyword,
            b.description
        FROM iosBookmarksStaging b",
        bookmark_kind = SyncedBookmarkKind::Bookmark as u8,
        folder_kind = SyncedBookmarkKind::Folder as u8,
        separator_kind = SyncedBookmarkKind::Separator as u8,

        ios_bookmark_type = IosBookmarkType::Bookmark as u8,
        ios_folder_type = IosBookmarkType::Folder as u8,
        ios_separator_type = IosBookmarkType::Separator as u8,

    );
}

// This could be a const &str, but it fits better here.
const POPULATE_MIRROR_STRUCTURE: &str = "
REPLACE INTO main.moz_bookmarks_synced_structure(guid, parentGuid, position)
    SELECT structure.child, structure.parent, structure.idx FROM ios.bookmarksBufferStructure structure
    WHERE EXISTS(
        SELECT 1 FROM iosBookmarksStaging stage
        WHERE stage.isLocal = 0
            AND stage.guid = structure.child
    );
REPLACE INTO main.moz_bookmarks_synced_structure(guid, parentGuid, position)
    SELECT structure.child, structure.parent, structure.idx FROM ios.bookmarksLocalStructure structure
    WHERE EXISTS(
        SELECT 1 FROM iosBookmarksStaging stage
        WHERE stage.isLocal != 0
            AND stage.guid = structure.child
    );
";

const CREATE_STAGING_TABLE: &str = "
    CREATE TEMP TABLE temp.iosBookmarksStaging(
        id INTEGER PRIMARY KEY,
        guid TEXT NOT NULL UNIQUE,
        type TINYINT NOT NULL,
        parentid TEXT,
        pos INT,
        title TEXT,
        description TEXT,
        bmkUri TEXT,
        keyword TEXT,
        date_added INTEGER NOT NULL,
        modified INTEGER NOT NULL,
        isLocal TINYINT NOT NULL
    )
";

lazy_static::lazy_static! {
    static ref POPULATE_STAGING: String = format!(
        "INSERT INTO temp.iosBookmarksStaging(
            guid,
            type,
            parentid,
            pos,
            title,
            description,
            bmkUri,
            keyword,
            date_added,
            modified,
            isLocal
        )
        SELECT
            b.guid,
            b.type,
            b.parentid,
            b.pos,
            b.title,
            b.description,
            b.bmkUri,
            b.keyword,
            b.date_added,
            b.server_modified,
            0
        FROM ios.bookmarksBuffer b
        WHERE NOT b.is_deleted
            AND b.type IN {valid_types}
            AND (b.bmkUri IS NULL OR length(b.bmkUri) < {url_length_max});

        REPLACE INTO temp.iosBookmarksStaging(
            guid,
            type,
            parentid,
            pos,
            title,
            description,
            bmkUri,
            keyword,
            date_added,
            modified,
            isLocal
        )
        SELECT
            l.guid,
            l.type,
            l.parentid,
            l.pos,
            l.title,
            l.description,
            l.bmkUri,
            l.keyword,
            l.date_added,
            l.local_modified,
            1
        FROM ios.bookmarksLocal l
        WHERE NOT l.is_deleted
            AND l.type IN {valid_types}
            AND (l.bmkUri IS NULL OR length(l.bmkUri) < {url_length_max})
            -- It's not clear if this matters
            AND l.guid NOT IN {roots};",
        valid_types = &*IOS_VALID_TYPES,
        url_length_max = URL_LENGTH_MAX,
        roots = ROOTS,
    );

    static ref FIXUP_MOZ_BOOKMARKS: String = format!(
        // Is there anything else?
        "UPDATE main.moz_bookmarks SET
           syncStatus = {unknown},
           syncChangeCounter = 0,
           lastModified = IFNULL((SELECT stage.modified FROM temp.iosBookmarksStaging stage
                                  WHERE stage.guid = main.moz_bookmarks.guid),
                                 now())",
        unknown = SyncStatus::Unknown as u8
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
        if let Err(e) = self.conn.execute_batch(self.sql) {
            log::error!("Failed to clean up after import! {}", e);
            log::debug!("  Failed query: {}", self.sql);
        }
    }
}

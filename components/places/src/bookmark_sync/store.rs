/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::record::{BookmarkItemRecord, BookmarkRecord, FolderRecord};
use crate::error::*;
use crate::storage::{bookmarks::maybe_truncate_title, get_meta, put_meta, URL_LENGTH_MAX};
use crate::types::{SyncedBookmarkKind, SyncedBookmarkValidity};
use rusqlite::Connection;
use sql_support::ConnExt;
use std::cell::Cell;
use std::result;
use std::time::Duration;
use sync15::telemetry;
use sync15::CollectionRequest;
use sync15::{ClientInfo, IncomingChangeset, OutgoingChangeset, ServerTimestamp, Store};
use url::Url;

static LAST_SYNC_META_KEY: &'static str = "bookmarks_last_sync_time";

pub struct BookmarksStore<'a> {
    pub db: &'a Connection,
    pub client_info: &'a Cell<Option<ClientInfo>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Staging {
    /// Incoming records set `needsMerge = true` in `moz_bookmarks_synced`,
    /// indicating the item has changes that we should merge and apply.
    Incoming,

    /// Outgoing records set `needsMerge = false` in `moz_bookmarks_synced`,
    /// indicating they've been uploaded and need to be written back.
    Outgoing,
}

// TODO: `impl<'a> dogear::Store for BookmarksStore<'a>`.

impl<'a> BookmarksStore<'a> {
    fn store_bookmark(
        &self,
        staging: Staging,
        modified: ServerTimestamp,
        b: BookmarkRecord,
    ) -> Result<()> {
        let (url, validity) = match self.maybe_store_url(b.url.as_ref()) {
            Ok(url) => (Some(url.into_string()), SyncedBookmarkValidity::Valid),
            Err(_) => (None, SyncedBookmarkValidity::Replace),
        };
        let needs_merge = staging == Staging::Incoming;
        self.db.execute_named_cached(
            r#"REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge, kind,
                                                 dateAdded, title, keyword, validity, placeId)
               VALUES(:guid, :parentGuid, :serverModified, :needsMerge, :kind,
                      :dateAdded, NULLIF(:title, ""), :keyword, :validity,
                      (SELECT id FROM moz_places
                       WHERE hash = hash(:url) AND
                             url = :url)"#,
            &[
                (":guid", &b.guid.as_ref()),
                (":parentGuid", &b.parent_guid.as_ref()),
                (":serverModified", &(modified.as_millis() as i64)),
                (":needsMerge", &needs_merge),
                (":kind", &SyncedBookmarkKind::Bookmark),
                (":dateAdded", &b.date_added),
                (":title", &maybe_truncate_title(&b.title)),
                (":keyword", &b.keyword),
                (":validity", &validity),
                (":url", &url),
            ],
        )?;
        Ok(())
    }

    fn store_folder(
        &self,
        staging: Staging,
        modified: ServerTimestamp,
        b: FolderRecord,
    ) -> Result<()> {
        let needs_merge = staging == Staging::Incoming;
        self.db.execute_named_cached(
            r#"REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge, kind,
                                                 dateAdded, title, validity)
               VALUES(:guid, :parentGuid, :serverModified, :needsMerge, :kind,
                      :dateAdded, NULLIF(:title, ""), :validity)"#,
            &[
                (":guid", &b.guid.as_ref()),
                (":parentGuid", &b.parent_guid.as_ref()),
                (":serverModified", &(modified.as_millis() as i64)),
                (":needsMerge", &needs_merge),
                (":kind", &SyncedBookmarkKind::Folder),
                (":dateAdded", &b.date_added),
                (":title", &maybe_truncate_title(&b.title)),
                (":validity", &SyncedBookmarkValidity::Valid),
            ],
        )?;
        for (position, child_guid) in b.children.iter().enumerate() {
            self.db.execute_named_cached(
                "REPLACE INTO moz_bookmarks_synced_structure(guid, parentGuid, position)
                 VALUES(:guid, :parentGuid, :position)",
                &[
                    (":guid", &child_guid),
                    (":parentGuid", &b.guid.as_ref()),
                    (":position", &(position as i64)),
                ],
            )?;
        }
        Ok(())
    }

    fn maybe_store_url(&self, url: Option<&String>) -> Result<Url> {
        if let Some(url) = url {
            let url = Url::parse(url)?;
            if url.as_str().len() > URL_LENGTH_MAX {
                return Err(ErrorKind::InvalidPlaceInfo(InvalidPlaceInfo::UrlTooLong).into());
            }
            self.db.execute_named_cached(
                "INSERT OR IGNORE INTO moz_places(guid, url, url_hash)
                 VALUES(IFNULL((SELECT guid FROM urls
                                WHERE url_hash = hash(:url) AND
                                      url = :url),
                        generate_guid()), :url, hash(:url))",
                &[(":url", &url.as_str())],
            )?;
            Ok(url)
        } else {
            Err(ErrorKind::InvalidPlaceInfo(InvalidPlaceInfo::NoUrl).into())
        }
    }
}

impl<'a> Store for BookmarksStore<'a> {
    #[inline]
    fn collection_name(&self) -> &'static str {
        "bookmarks"
    }

    fn apply_incoming(
        &self,
        inbound: IncomingChangeset,
        incoming_telemetry: &mut telemetry::EngineIncoming,
    ) -> result::Result<OutgoingChangeset, failure::Error> {
        // Stage all incoming items.
        let timestamp = inbound.timestamp;
        let mut tx = self
            .db
            .time_chunked_transaction(Duration::from_millis(1000))?;
        for incoming in inbound.changes {
            let item = BookmarkItemRecord::from_payload(incoming.0)?;
            match item {
                BookmarkItemRecord::Bookmark(b) => {
                    self.store_bookmark(Staging::Incoming, timestamp, b)?
                }
                BookmarkItemRecord::Folder(f) => {
                    self.store_folder(Staging::Incoming, timestamp, f)?
                }
                _ => unimplemented!("TODO: Store other types"),
            }
            tx.maybe_commit()?;
        }
        tx.commit()?;

        // write the timestamp now, so if we are interrupted merging or
        // creating outgoing changesets we don't need to re-download the same
        // records.
        put_meta(self.db, LAST_SYNC_META_KEY, &(timestamp.as_millis() as i64))?;

        // TODO: Run the merge.

        let mut outgoing = OutgoingChangeset::new(self.collection_name().into(), inbound.timestamp);
        // TODO: Fetch staged items from `itemsToUpload`.
        Ok(outgoing)
    }

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: &[String],
    ) -> result::Result<(), failure::Error> {
        unimplemented!("TODO: Write sync records back to the mirror and update sync statuses");
    }

    fn get_collection_request(&self) -> result::Result<CollectionRequest, failure::Error> {
        let since = get_meta::<i64>(self.db, LAST_SYNC_META_KEY)?
            .map(|millis| ServerTimestamp(millis as f64 / 1000.0))
            .unwrap_or_default();
        Ok(CollectionRequest::new(self.collection_name())
            .full()
            .newer_than(since))
    }

    fn reset(&self) -> result::Result<(), failure::Error> {
        unimplemented!("TODO: Wipe staged items and reset sync statuses");
    }

    fn wipe(&self) -> result::Result<(), failure::Error> {
        log::warn!("not implemented");
        Ok(())
    }
}

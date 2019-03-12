/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::record::{
    BookmarkItemRecord, BookmarkRecord, FolderRecord, QueryRecord, SeparatorRecord,
};
use crate::error::*;
use crate::storage::{
    bookmarks::{maybe_truncate_title, BookmarkRootGuid},
    get_meta, put_meta, URL_LENGTH_MAX,
};
use crate::types::{
    BookmarkType, SyncGuid, SyncStatus, SyncedBookmarkKind, SyncedBookmarkValidity, Timestamp,
};
use dogear::{
    self, Content, Deletion, IntoTree, Item, LogLevel, MergedDescendant, Tree, UploadReason,
};
use lazy_static::lazy_static;
use rusqlite::{Connection, NO_PARAMS};
use sql_support::{self, ConnExt};
use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::result;
use std::time::Duration;
use sync15::{
    telemetry, ClientInfo, CollectionRequest, IncomingChangeset, OutgoingChangeset, Payload,
    ServerTimestamp, Store,
};
use url::Url;

static LAST_SYNC_META_KEY: &'static str = "bookmarks_last_sync_time";

lazy_static! {
    static ref LOCAL_ROOTS_AS_SQL_SET: String = {
        // phew - this seems more complicated then it should be.
        let roots_as_strings: Vec<String> = BookmarkRootGuid::user_roots().iter().map(|g| format!("'{}'", g.as_guid())).collect();
        roots_as_strings.join(",")
    };

    static ref LOCAL_ITEMS_SQL_FRAGMENT: String = {
        format!(
            "localItems(id, guid, parentId, parentGuid, position, type, title,
                     parentTitle, placeId, dateAdded, lastModified, syncChangeCounter,
                     isSyncable, level) AS (
            SELECT b.id, b.guid, p.id, p.guid, b.position, b.type, b.title, p.title,
                   b.fk, b.dateAdded, b.lastModified, b.syncChangeCounter,
                   b.guid IN ({user_content_roots}), 0
            FROM moz_bookmarks b
            JOIN moz_bookmarks p ON p.id = b.parent
            WHERE b.guid <> '{tags_guid}' AND
                  p.guid = '{root_guid}'
            UNION ALL
            SELECT b.id, b.guid, s.id, s.guid, b.position, b.type, b.title, s.title,
                   b.fk, b.dateAdded, b.lastModified, b.syncChangeCounter,
                   s.isSyncable, s.level + 1
            FROM moz_bookmarks b
            JOIN localItems s ON s.id = b.parent
            WHERE b.guid <> '{root_guid}')",
            user_content_roots = *LOCAL_ROOTS_AS_SQL_SET,
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref(),
            tags_guid = "_tags_" // XXX - need tags!
        )
    };
}

pub struct BookmarksStore<'a> {
    pub db: &'a Connection,
    pub client_info: &'a Cell<Option<ClientInfo>>,
    local_time: Timestamp,
    remote_time: ServerTimestamp,
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

struct Driver;

impl dogear::Driver for Driver {
    fn generate_new_guid(&self, _invalid_guid: &dogear::Guid) -> dogear::Result<dogear::Guid> {
        Ok(SyncGuid::new().into())
    }

    fn log_level(&self) -> LogLevel {
        LogLevel::Silent
    }

    fn log(&self, _level: LogLevel, _args: fmt::Arguments) {}
}

impl<'a> dogear::Store<Error> for BookmarksStore<'a> {
    /// Builds a fully rooted, consistent tree from all local items and
    /// tombstones.
    fn fetch_local_tree(&self) -> Result<Tree> {
        let mut builder = Tree::with_root(Item::root());

        let sql = format!(
            r#"
            WITH RECURSIVE
            {local_items_fragment}
            SELECT s.id, s.guid, s.parentGuid, {kind} AS kind,
                   s.lastModified / 1000 AS localModified, s.syncChangeCounter
            FROM localItems s
            ORDER BY s.level, s.parentId, s.position"#,
            local_items_fragment = *LOCAL_ITEMS_SQL_FRAGMENT,
            kind = type_to_kind("s.type", "s.placeId"),
        );
        let mut stmt = self.db.prepare(&sql)?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(result) = results.next() {
            let row = result?;
            let guid = row.get_checked::<_, SyncGuid>("guid")?;
            let kind = SyncedBookmarkKind::from_u8(row.get_checked("kind")?)?;
            let mut item = Item::new(guid.into(), kind.into());
            // Note that this doesn't account for local clock skew.
            let age = row
                .get_checked::<_, Timestamp>("localModified")
                .unwrap_or_default()
                .duration_since(self.local_time)
                .unwrap_or_default();
            item.age = age.as_secs() as i64 * 1000 + i64::from(age.subsec_millis());
            item.needs_merge = row.get_checked::<_, u32>("syncChangeCounter")? > 0;
            let parent_guid = row.get_checked::<_, SyncGuid>("parentGuid")?;
            builder.item(item)?.by_structure(&parent_guid.into())?;
        }

        let mut tree = builder.into_tree()?;

        // Note tombstones for locally deleted items.
        let mut stmt = self.db.prepare("SELECT guid FROM moz_bookmarks_deleted")?;
        let rows = stmt.query_and_then(NO_PARAMS, |row| row.get_checked::<_, SyncGuid>("guid"))?;
        for row in rows {
            let guid = row?;
            tree.note_deleted(guid.into());
        }

        Ok(tree)
    }

    /// Fetches content info for all "new" and "unknown" local items that
    /// haven't been synced. We'll try to dedupe them to changed remote items
    /// with similar contents and different GUIDs.
    fn fetch_new_local_contents(&self) -> Result<HashMap<dogear::Guid, Content>> {
        let mut contents = HashMap::new();

        let sql = format!(
            r#"
            SELECT b.guid, b.type, IFNULL(b.title, "") AS title, h.url,
                   b.position
            FROM moz_bookmarks b
            JOIN moz_bookmarks p ON p.id = b.parent
            LEFT JOIN moz_places h ON h.id = b.fk
            LEFT JOIN moz_bookmarks_synced v ON v.guid = b.guid
            WHERE v.guid IS NULL AND
                  p.guid <> '{root_guid}' AND
                  b.syncStatus <> {sync_status}"#,
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref(),
            sync_status = SyncStatus::Normal as u8
        );
        let mut stmt = self.db.prepare(&sql)?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(result) = results.next() {
            let row = result?;
            let typ = match BookmarkType::from_u8(row.get_checked("type")?) {
                Some(t) => t,
                None => continue,
            };
            let content = match typ {
                BookmarkType::Bookmark => {
                    let title = row.get_checked("title")?;
                    let url_href = row.get_checked("url")?;
                    Content::Bookmark { title, url_href }
                }
                BookmarkType::Folder => {
                    let title = row.get_checked("title")?;
                    Content::Folder { title }
                }
                BookmarkType::Separator => {
                    let position = row.get_checked("position")?;
                    Content::Separator { position }
                }
            };
            let guid = row.get_checked::<_, SyncGuid>("guid")?;
            contents.insert(guid.into(), content);
        }

        Ok(contents)
    }

    /// Builds a fully rooted tree from all synced items and tombstones.
    fn fetch_remote_tree(&self) -> Result<Tree> {
        let mut builder = Tree::with_root(Item::root());

        let sql = format!(
            "
            SELECT guid, parentGuid, serverModified, kind, needsMerge, validity
            FROM moz_bookmarks_synced
            WHERE NOT isDeleted AND
                  guid <> '{root_guid}'",
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref()
        );
        let mut stmt = self.db.prepare(&sql)?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(result) = results.next() {
            let row = result?;
            let guid = row.get_checked::<_, SyncGuid>("guid")?;
            let kind = SyncedBookmarkKind::from_u8(row.get_checked("kind")?)?;
            let mut item = Item::new(guid.into(), kind.into());
            let age = ServerTimestamp(row.get_checked::<_, f64>("serverModified").unwrap_or(0f64))
                .duration_since(self.remote_time)
                .unwrap_or_default();
            item.age = age.as_secs() as i64 * 1000 + i64::from(age.subsec_millis());
            item.needs_merge = row.get_checked("needsMerge")?;
            item.validity = SyncedBookmarkValidity::from_u8(row.get_checked("validity")?)?.into();

            let p = builder.item(item)?;
            if let Some(parent_guid) = row.get_checked::<_, Option<SyncGuid>>("parentGuid")? {
                p.by_parent_guid(parent_guid.into())?;
            }
        }

        let sql = format!(
            "
            SELECT guid, parentGuid FROM moz_bookmarks_synced_structure
            WHERE guid <> '{root_guid}'
            ORDER BY parentGuid, position",
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref()
        );
        let mut stmt = self.db.prepare(&sql)?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(result) = results.next() {
            let row = result?;
            let guid = row.get_checked::<_, SyncGuid>("guid")?;
            let parent_guid = row.get_checked::<_, SyncGuid>("parentGuid")?;
            builder
                .parent_for(&guid.into())
                .by_children(&parent_guid.into())?;
        }

        let mut tree = builder.into_tree()?;

        // Note tombstones for remotely deleted items.
        let mut stmt = self
            .db
            .prepare("SELECT guid FROM moz_bookmarks_synced WHERE isDeleted AND needsMerge")?;
        let rows = stmt.query_and_then(NO_PARAMS, |row| row.get_checked::<_, SyncGuid>("guid"))?;
        for row in rows {
            let guid = row?;
            tree.note_deleted(guid.into());
        }

        Ok(tree)
    }

    /// Fetches content info for all synced items that changed since the last
    /// sync and don't exist locally.
    fn fetch_new_remote_contents(&self) -> Result<HashMap<dogear::Guid, Content>> {
        let mut contents = HashMap::new();

        let sql = format!(
            r#"
            SELECT v.guid, v.kind, IFNULL(v.title, "") AS title, h.url,
                   s.position
            FROM moz_bookmarks_synced v
            JOIN moz_bookmarks_synced_structure s ON s.guid = v.guid
            LEFT JOIN moz_places h ON h.id = v.placeId
            LEFT JOIN moz_bookmarks b ON b.guid = v.guid
            WHERE NOT v.isDeleted AND
                  v.needsMerge AND
                  b.guid IS NULL AND
                  s.parentGuid <> '{root_guid}'"#,
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref()
        );
        let mut stmt = self.db.prepare(&sql)?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(result) = results.next() {
            let row = result?;
            let content = match SyncedBookmarkKind::from_u8(row.get_checked("kind")?)? {
                SyncedBookmarkKind::Bookmark | SyncedBookmarkKind::Query => {
                    let title = row.get_checked("title")?;
                    let url_href = row.get_checked("url")?;
                    Content::Bookmark { title, url_href }
                }
                SyncedBookmarkKind::Folder => {
                    let title = row.get_checked("title")?;
                    Content::Folder { title }
                }
                SyncedBookmarkKind::Separator => {
                    let position = row.get_checked("position")?;
                    Content::Separator { position }
                }
                _ => continue,
            };
            let guid = row.get_checked::<_, SyncGuid>("guid")?;
            contents.insert(guid.into(), content);
        }

        Ok(contents)
    }

    fn apply<'t>(
        &self,
        descendants: Vec<MergedDescendant<'t>>,
        deletions: Vec<Deletion>,
    ) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        let result = self
            .update_local_items(descendants, deletions)
            .and_then(|_| self.stage_local_items_to_upload())
            .and_then(|_| {
                self.db.execute_batch(
                    "
                    DELETE FROM mergedTree;
                    DELETE FROM idsToWeaklyUpload;",
                )?;
                Ok(())
            });
        match result {
            Ok(_) => tx.commit()?,
            Err(_) => tx.rollback()?,
        }
        result
    }
}

impl<'a> BookmarksStore<'a> {
    fn store_bookmark(
        &self,
        staging: Staging,
        modified: ServerTimestamp,
        b: BookmarkRecord,
    ) -> Result<()> {
        let (url, validity) = match self.maybe_store_url(b.url.as_ref()) {
            Ok(url) => (Some(url.into_string()), SyncedBookmarkValidity::Valid),
            Err(e) => {
                log::warn!("Incoming bookmark has an invalid URL: {:?}", e);
                (None, SyncedBookmarkValidity::Replace)
            }
        };
        let needs_merge = staging == Staging::Incoming;
        self.db.execute_named_cached(
            r#"REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge, kind,
                                                 dateAdded, title, keyword, validity, placeId)
               VALUES(:guid, :parentGuid, :serverModified, :needsMerge, :kind,
                      :dateAdded, NULLIF(:title, ""), :keyword, :validity,
                      -- XXX - when url is null we still fail below when we call hash()???
                      CASE WHEN :url ISNULL
                      THEN NULL
                      ELSE (SELECT id FROM moz_places
                            WHERE url_hash = hash(:url) AND
                            url = :url)
                      END
                      )"#,
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
                 VALUES(IFNULL((SELECT guid FROM moz_places
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

    pub fn apply_payload(
        &self,
        timestamp: ServerTimestamp,
        payload: sync15::Payload,
    ) -> Result<()> {
        let item = BookmarkItemRecord::from_payload(payload)?;
        match item {
            BookmarkItemRecord::Bookmark(b) => {
                self.store_bookmark(Staging::Incoming, timestamp, b)?
            }
            BookmarkItemRecord::Folder(f) => self.store_folder(Staging::Incoming, timestamp, f)?,
            _ => unimplemented!("TODO: Store other types"),
        }
        Ok(())
    }

    fn has_changes(&self) -> Result<bool> {
        // In the first subquery, we check incoming items with needsMerge = true
        // except the tombstones who don't correspond to any local bookmark because
        // we don't store them yet, hence never "merged" (see bug 1343103).
        let sql = format!(
            "
            SELECT
              EXISTS (
               SELECT 1
               FROM moz_bookmarks_synced v
               LEFT JOIN moz_bookmarks b ON v.guid = b.guid
               WHERE v.needsMerge AND
               (NOT v.isDeleted OR b.guid NOT NULL)
              ) OR EXISTS (
               WITH RECURSIVE
               {}
               SELECT 1
               FROM localItems
               WHERE syncChangeCounter > 0
              ) OR EXISTS (
               SELECT 1
               FROM moz_bookmarks_deleted
              )
              AS hasChanges
        ",
            *LOCAL_ITEMS_SQL_FRAGMENT
        );
        Ok(self
            .db
            .try_query_row(
                &sql,
                &[],
                |row| -> rusqlite::Result<_> { Ok(row.get_checked::<_, bool>(0)?) },
                false,
            )?
            .unwrap_or(false))
    }

    /// If the local roots aren't valid the merger will have a bad time.
    fn valid_local_roots(&self) -> Result<bool> {
        let sql = "
            SELECT EXISTS(SELECT 1 FROM moz_bookmarks
                    WHERE guid = '{root_guid}' AND
                          parent = NULL) AND
             (SELECT COUNT(*) FROM moz_bookmarks b
              JOIN moz_bookmarks p ON p.id = b.parent
              WHERE b.guid IN {local_roots} AND
                    p.guid = '{root_guid}') = {num_user_roots} AS areValid";
        Ok(self
            .db
            .try_query_row(
                &sql,
                &[],
                |row| -> rusqlite::Result<_> { Ok(row.get_checked::<_, bool>(0)?) },
                false,
            )?
            .unwrap_or(false))
    }

    /// Builds a temporary table with the merge states of all nodes in the merged
    /// tree, then updates the local tree to match the merged tree.
    ///
    /// Conceptually, we examine the merge state of each item, and either leave the
    /// item unchanged, upload the local side, apply the remote side, or apply and
    /// then reupload the remote side with a new structure.
    fn update_local_items<'t>(
        &self,
        descendants: Vec<MergedDescendant<'t>>,
        deletions: Vec<Deletion>,
    ) -> Result<()> {
        // First, insert rows for all merged descendants.
        sql_support::each_sized_chunk(
            &descendants,
            sql_support::default_max_variable_number() / 4,
            |chunk, _| -> Result<()> {
                // We can't avoid allocating here, since we're binding four
                // parameters per descendant. Rust's `SliceConcatExt::concat`
                // is semantically equivalent, but requires a second allocation,
                // which we _can_ avoid.
                let mut params = Vec::with_capacity(chunk.len() * 4);
                for d in chunk.iter() {
                    params.push(
                        d.merged_node
                            .merge_state
                            .local_node()
                            .map(|node| node.guid.as_str()),
                    );
                    params.push(
                        d.merged_node
                            .merge_state
                            .remote_node()
                            .map(|node| node.guid.as_str()),
                    );
                    params.push(Some(d.merged_node.guid.as_str()));
                    params.push(Some(d.merged_parent_node.guid.as_str()));
                }
                self.db.execute(&format!(
                    "
                    INSERT INTO mergedTree(localGuid, remoteGuid, mergedGuid, mergedParentGuid, level,
                                           position, useRemote, shouldUpload)
                    VALUES {}",
                    sql_support::repeat_display(chunk.len(), ",", |index, f| {
                        let d = &chunk[index];
                        write!(f, "(?, ?, ?, ?, {}, {}, {}, {})",
                            d.level, d.position, d.merged_node.merge_state.should_apply(),
                            d.merged_node.merge_state.upload_reason() != UploadReason::None)
                    })
                ), &params)?;
                Ok(())
            },
        )?;

        // Next, insert rows for deletions. Unlike Desktop, there's no
        // `noteItemRemoved` trigger on `itemsToRemove`, since we don't support
        // observer notifications.
        sql_support::each_chunk(&deletions, |chunk, _| -> Result<()> {
            self.db.execute(
                &format!(
                    "
                    INSERT INTO itemsToRemove(guid, localLevel, shouldUploadTombstone)
                    VALUES {}",
                    sql_support::repeat_display(chunk.len(), ",", |index, f| {
                        let d = &chunk[index];
                        write!(f, "(?, {}, {})", d.local_level, d.should_upload_tombstone)
                    })
                ),
                chunk.iter().map(|d| d.guid.as_str()),
            )?;
            Ok(())
        })?;

        // "Deleting" from `itemsToMerge` fires the `insertNewLocalItems` and
        // `updateExistingLocalItems` triggers.
        self.db.execute_batch("DELETE FROM itemsToMerge")?;

        // "Deleting" from `structureToMerge` fires the `updateLocalStructure`
        // trigger.
        self.db.execute_batch("DELETE FROM structureToMerge")?;

        self.db.execute_batch("DELETE FROM itemsToRemove")?;

        self.db.execute_batch("DELETE FROM relatedIdsToReupload")?;

        Ok(())
    }

    /// Stores a snapshot of all locally changed items in a temporary table for
    /// upload. This is called from within the merge transaction, to ensure that
    /// changes made during the sync don't cause us to upload inconsistent
    /// records.
    ///
    /// Conceptually, `itemsToUpload` is a transient "view" of locally changed
    /// items. The local change counter is the persistent record of items that
    /// we need to upload, so, if upload is interrupted or fails, we'll stage
    /// the items again on the next sync.
    fn stage_local_items_to_upload(&self) -> Result<()> {
        // Stage remotely changed items with older local creation dates. These are
        // tracked "weakly": if the upload is interrupted or fails, we won't
        // reupload the record on the next sync.
        self.db.execute_batch(
            r#"
            INSERT OR IGNORE INTO idsToWeaklyUpload(id)
            SELECT b.id FROM moz_bookmarks b
            JOIN mergedTree r ON r.mergedGuid = b.guid
            JOIN moz_bookmarks_synced v ON v.guid = r.remoteGuid
            WHERE r.useRemote AND
                  /* "b.dateAdded" is in microseconds; "v.dateAdded" is in
                     milliseconds. */
                  b.dateAdded < v.dateAdded"#,
        )?;

        // Stage remaining locally changed items for upload.
        self.db.execute_batch(&format!(
            "
            WITH RECURSIVE
            {local_items_fragment}
            INSERT INTO itemsToUpload(id, guid, syncChangeCounter, parentGuid,
                                      parentTitle, dateAdded, type, title, placeId,
                                      isQuery, url, keyword, position, tagFolderName)
            SELECT s.id, s.guid, s.syncChangeCounter, s.parentGuid, s.parentTitle,
                   s.dateAdded, s.type, s.title, s.placeId,
                   IFNULL(substr(h.url, 1, 6) = 'place:', 0) AS isQuery,
                   h.url,
                   NULL AS keyword,
                   s.position,
                   NULL AS tagFolderName
            FROM localItems s
            LEFT JOIN moz_places h ON h.id = s.placeId
            LEFT JOIN idsToWeaklyUpload w ON w.id = s.id
            WHERE s.syncChangeCounter >= 1 OR
                  w.id NOT NULL",
            local_items_fragment = *LOCAL_ITEMS_SQL_FRAGMENT,
        ))?;

        // Record the child GUIDs of locally changed folders, which we use to
        // populate the `children` array in the record.
        self.db.execute_batch(
            "
            INSERT INTO structureToUpload(guid, parentId, position)
            SELECT b.guid, b.parent, b.position FROM moz_bookmarks b
            JOIN itemsToUpload o ON o.id = b.parent",
        )?;

        // Finally, stage tombstones for deleted items.
        self.db.execute_batch(
            "
            INSERT OR IGNORE INTO itemsToUpload(guid, syncChangeCounter, isDeleted)
            SELECT guid, 1, 1 FROM moz_bookmarks_deleted",
        )?;

        Ok(())
    }

    /// Inflates Sync records for all staged outgoing items.
    fn fetch_outgoing_records(&self, timestamp: ServerTimestamp) -> Result<OutgoingChangeset> {
        let mut outgoing = OutgoingChangeset::new(self.collection_name().into(), timestamp);
        let mut child_guids_by_local_parent_id: HashMap<i64, Vec<SyncGuid>> = HashMap::new();

        let mut stmt = self.db.prepare(
            "SELECT parentId, guid FROM structureToUpload
             ORDER BY parentId, position",
        )?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(result) = results.next() {
            let row = result?;
            let local_parent_id = row.get_checked::<_, i64>("parentId")?;
            let child_guid = row.get_checked::<_, SyncGuid>("guid")?;
            let child_guids = child_guids_by_local_parent_id
                .entry(local_parent_id)
                .or_default();
            child_guids.push(child_guid);
        }

        let mut stmt = self.db.prepare(
            r#"SELECT id, syncChangeCounter, guid, isDeleted, type, isQuery,
                      tagFolderName, keyword, url, IFNULL(title, "") AS title,
                      position, parentGuid,
                      IFNULL(parentTitle, "") AS parentTitle, dateAdded
               FROM itemsToUpload"#,
        )?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(result) = results.next() {
            let row = result?;
            let guid = row.get_checked::<_, SyncGuid>("guid")?;
            let is_deleted = row.get_checked::<_, bool>("isDeleted")?;
            if is_deleted {
                outgoing.changes.push(Payload::new_tombstone(guid.0));
                continue;
            }
            let parent_guid = row.get_checked::<_, SyncGuid>("parentGuid")?;
            let parent_title = row.get_checked::<_, String>("parentTitle")?;
            let date_added = row.get_checked::<_, i64>("dateAdded")?;
            let record: BookmarkItemRecord =
                match BookmarkType::from_u8(row.get_checked("type")?).unwrap() {
                    BookmarkType::Bookmark => {
                        let is_query = row.get_checked::<_, bool>("isQuery")?;
                        let title = row.get_checked::<_, String>("title")?;
                        let url = row.get_checked::<_, String>("url")?;
                        if is_query {
                            QueryRecord {
                                guid,
                                parent_guid: Some(parent_guid),
                                has_dupe: true,
                                parent_title: Some(parent_title),
                                date_added: Some(date_added),
                                title: Some(title),
                                url: Some(url),
                                tag_folder_name: None,
                            }
                            .into()
                        } else {
                            BookmarkRecord {
                                guid,
                                parent_guid: Some(parent_guid),
                                has_dupe: true,
                                parent_title: Some(parent_title),
                                date_added: Some(date_added),
                                title: Some(title),
                                url: Some(url),
                                keyword: None,
                                tags: Vec::new(),
                            }
                            .into()
                        }
                    }
                    BookmarkType::Folder => {
                        let title = row.get_checked::<_, String>("title")?;
                        let local_id = row.get_checked::<_, i64>("id")?;
                        let children = child_guids_by_local_parent_id
                            .remove(&local_id)
                            .unwrap_or_default();
                        FolderRecord {
                            guid,
                            parent_guid: Some(parent_guid),
                            has_dupe: true,
                            parent_title: Some(parent_title),
                            date_added: Some(date_added),
                            title: Some(title),
                            children,
                        }
                        .into()
                    }
                    BookmarkType::Separator => {
                        let position = row.get_checked::<_, i64>("position")?;
                        SeparatorRecord {
                            guid,
                            parent_guid: Some(parent_guid),
                            has_dupe: true,
                            parent_title: Some(parent_title),
                            date_added: Some(date_added),
                            position: Some(position),
                        }
                        .into()
                    }
                };
            outgoing.changes.push(Payload::from_record(record)?);
        }

        Ok(outgoing)
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
        use dogear::Store;

        // Stage all incoming items.
        let timestamp = inbound.timestamp;
        let mut tx = self
            .db
            .time_chunked_transaction(Duration::from_millis(1000))?;
        for incoming in inbound.changes {
            self.apply_payload(timestamp, incoming.0)?;
            tx.maybe_commit()?;
        }
        tx.commit()?;

        // write the timestamp now, so if we are interrupted merging or
        // creating outgoing changesets we don't need to re-download the same
        // records.
        put_meta(self.db, LAST_SYNC_META_KEY, &(timestamp.as_millis() as i64))?;

        // Merge and stage outgoing items.
        self.merge_with_driver(&Driver)?;

        let outgoing = self.fetch_outgoing_records(inbound.timestamp)?;
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

fn type_to_kind<'a>(typ: &'a str, place_id: &'a str) -> TypeToKind<'a> {
    TypeToKind { typ, place_id }
}

/// A helper that interpolates a SQL expression for converting Places item types
/// to Sync record kinds. `typ` is the name of the `moz_bookmarks.type` column.
/// `place_id` is the name of the `moz_bookmarks.fk` column.
struct TypeToKind<'a> {
    typ: &'a str,
    place_id: &'a str,
}

impl<'a> fmt::Display for TypeToKind<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            r#"(CASE {typ}
            WHEN {bookmark_type} THEN (
             CASE SUBSTR((SELECT h.url FROM moz_places h
                          WHERE h.id = {place_id}), 1, 6)
             /* Queries are bookmarks with a "place:" URL scheme. */
             WHEN 'place:' THEN {query_kind}
             ELSE {bookmark_kind} END)
           WHEN {folder_type} THEN {folder_kind}
           ELSE {separator_kind} END)"#,
            typ = self.typ,
            bookmark_type = BookmarkType::Bookmark as u8,
            place_id = self.place_id,
            bookmark_kind = SyncedBookmarkKind::Bookmark as u8,
            folder_type = BookmarkType::Folder as u8,
            folder_kind = SyncedBookmarkKind::Folder as u8,
            separator_kind = SyncedBookmarkKind::Separator as u8,
            query_kind = SyncedBookmarkKind::Query as u8
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::{test::new_mem_api, ConnectionType};
    use crate::bookmark_sync::store::BookmarksStore;
    use crate::tests::{
        assert_json_tree as assert_local_json_tree, insert_json_tree as insert_local_json_tree,
    };
    use dogear::{Store as DogearStore, Validity};
    use serde_json::json;
    use sync15::Store as SyncStore;

    use std::cell::Cell;
    use sync15::Payload;

    #[test]
    fn test_fetch_remote_tree() -> Result<()> {
        let records = vec![
            json!({
                "id": "qqVTRWhLBOu3",
                "type": "bookmark",
                "parentid": BookmarkRootGuid::Unfiled.as_guid(),
                "parentName": "Unfiled Bookmarks",
                "dateAdded": 1381542355843u64,
                "title": "The title",
                "bmkUri": "https://example.com",
                "tags": [],
            }),
            json!({
                "id": BookmarkRootGuid::Unfiled.as_guid(),
                "type": "folder",
                "parentid": BookmarkRootGuid::Root.as_guid(),
                "parentName": "",
                "dateAdded": 0,
                "title": "Unfiled Bookmarks",
                "children": ["qqVTRWhLBOu3"],
                "tags": [],
            }),
        ];

        let api = new_mem_api();
        let conn = api.open_connection(ConnectionType::Sync)?;

        // suck records into the store.
        let store = BookmarksStore {
            db: &conn,
            client_info: &Cell::new(None),
            local_time: Timestamp::now(),
            remote_time: ServerTimestamp(0.0),
        };

        for record in records {
            let payload = Payload::from_json(record).unwrap();
            store.apply_payload(ServerTimestamp(0.0), payload)?;
        }

        let tree = store.fetch_remote_tree()?;

        // should be each user root, plus the real root, plus the bookmark we added.
        assert_eq!(
            tree.guids().count(),
            BookmarkRootGuid::user_roots().len() + 2
        );

        let node = tree
            .node_for_guid(&"qqVTRWhLBOu3".into())
            .expect("should exist");
        assert_eq!(node.needs_merge, true);
        assert_eq!(node.validity, Validity::Valid);
        assert_eq!(node.level(), 2);
        assert_eq!(node.is_syncable(), true);

        let node = tree
            .node_for_guid(&BookmarkRootGuid::Unfiled.as_guid().into())
            .expect("should exist");
        assert_eq!(node.needs_merge, true);
        assert_eq!(node.validity, Validity::Valid);
        assert_eq!(node.level(), 1);
        assert_eq!(node.is_syncable(), true);

        let node = tree
            .node_for_guid(&BookmarkRootGuid::Menu.as_guid().into())
            .expect("should exist");
        assert_eq!(node.needs_merge, false);
        assert_eq!(node.validity, Validity::Valid);
        assert_eq!(node.level(), 1);
        assert_eq!(node.is_syncable(), true);

        let node = tree
            .node_for_guid(&BookmarkRootGuid::Root.as_guid().into())
            .expect("should exist");
        assert_eq!(node.needs_merge, false);
        assert_eq!(node.validity, Validity::Valid);
        assert_eq!(node.level(), 0);
        assert_eq!(node.is_syncable(), false);

        // We should have changes.
        assert_eq!(store.has_changes().unwrap(), true);
        Ok(())
    }

    #[test]
    fn test_fetch_local_tree() -> Result<()> {
        let api = new_mem_api();
        let conn = api.open_connection(ConnectionType::Sync)?;

        conn.execute("UPDATE moz_bookmarks SET syncChangeCounter = 0", NO_PARAMS)
            .expect("should work");

        insert_local_json_tree(
            &conn,
            json!({
                "guid": &BookmarkRootGuid::Unfiled.as_guid(),
                "children": [
                    {
                        "guid": "bookmark1___",
                        "title": "the bookmark",
                        "url": "https://www.example.com/"
                    },
                ]
            }),
        );

        let store = BookmarksStore {
            db: &conn,
            client_info: &Cell::new(None),
            local_time: Timestamp::now(),
            remote_time: ServerTimestamp(0.0),
        };
        let tree = store.fetch_local_tree()?;

        // should be each user root, plus the real root, plus the bookmark we added.
        assert_eq!(
            tree.guids().count(),
            BookmarkRootGuid::user_roots().len() + 2
        );

        let node = tree
            .node_for_guid(&"bookmark1___".into())
            .expect("should exist");
        assert_eq!(node.needs_merge, true);
        assert_eq!(node.level(), 2);
        assert_eq!(node.is_syncable(), true);

        let node = tree
            .node_for_guid(&BookmarkRootGuid::Unfiled.as_guid().into())
            .expect("should exist");
        assert_eq!(node.needs_merge, true);
        assert_eq!(node.level(), 1);
        assert_eq!(node.is_syncable(), true);

        let node = tree
            .node_for_guid(&BookmarkRootGuid::Menu.as_guid().into())
            .expect("should exist");
        assert_eq!(node.needs_merge, false);
        assert_eq!(node.level(), 1);
        assert_eq!(node.is_syncable(), true);

        let node = tree
            .node_for_guid(&BookmarkRootGuid::Root.as_guid().into())
            .expect("should exist");
        assert_eq!(node.needs_merge, false);
        assert_eq!(node.level(), 0);
        assert_eq!(node.is_syncable(), false);

        // We should have changes.
        assert_eq!(store.has_changes().unwrap(), true);
        Ok(())
    }

    #[test]
    fn test_apply() -> Result<()> {
        let api = new_mem_api();
        let conn = api.open_connection(ConnectionType::Sync)?;

        conn.execute("UPDATE moz_bookmarks SET syncChangeCounter = 0", NO_PARAMS)
            .expect("should work");

        insert_local_json_tree(
            &conn,
            json!({
                "guid": &BookmarkRootGuid::Unfiled.as_guid(),
                "children": [
                    {
                        "guid": "bookmarkAAAA",
                        "title": "A",
                        "url": "http://example.com/a",
                    },
                    {
                        "guid": "bookmarkBBBB",
                        "title": "B",
                        "url": "http://example.com/b",
                    },
                ]
            }),
        );

        let records = vec![
            json!({
                "id": "bookmarkCCCC",
                "type": "bookmark",
                "parentid": BookmarkRootGuid::Menu.as_guid(),
                "parentName": "menu",
                "dateAdded": 1552183116885u64,
                "title": "C",
                "bmkUri": "http://example.com/c",
                "tags": [],
            }),
            json!({
                "id": BookmarkRootGuid::Menu.as_guid(),
                "type": "folder",
                "parentid": BookmarkRootGuid::Root.as_guid(),
                "parentName": "",
                "dateAdded": 0,
                "title": "menu",
                "children": ["bookmarkCCCC"],
            }),
        ];

        let mut store = BookmarksStore {
            db: &conn,
            client_info: &Cell::new(None),
            local_time: Timestamp::now(),
            remote_time: ServerTimestamp(0.0),
        };

        let mut incoming =
            IncomingChangeset::new(store.collection_name().to_string(), ServerTimestamp(0.0));
        for record in records {
            let payload = Payload::from_json(record).unwrap();
            incoming.changes.push((payload, ServerTimestamp(0.0)));
        }

        let mut outgoing = store
            .apply_incoming(incoming, &mut telemetry::EngineIncoming::new())
            .expect("Should apply incoming and stage outgoing records");
        outgoing.changes.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(
            outgoing.changes.iter().map(|p| &p.id).collect::<Vec<_>>(),
            vec![
                "bookmarkAAAA",
                "bookmarkBBBB",
                &BookmarkRootGuid::Unfiled.as_guid().as_ref()
            ]
        );

        assert_local_json_tree(
            &conn,
            &BookmarkRootGuid::Root.as_guid(),
            json!({
                "guid": &BookmarkRootGuid::Root.as_guid(),
                "children": [
                    {
                        "guid": &BookmarkRootGuid::Menu.as_guid(),
                        "children": [
                            {
                                "guid": "bookmarkCCCC",
                                "title": "C",
                                "url": "http://example.com/c",
                                "date_added": Timestamp(1552183116885),
                            },
                        ],
                    },
                    {
                        "guid": &BookmarkRootGuid::Toolbar.as_guid(),
                        "children": [],
                    },
                    {
                        "guid": &BookmarkRootGuid::Unfiled.as_guid(),
                        "children": [
                            {
                                "guid": "bookmarkAAAA",
                                "title": "A",
                                "url": "http://example.com/a",
                            },
                            {
                                "guid": "bookmarkBBBB",
                                "title": "B",
                                "url": "http://example.com/b",
                            },
                        ],
                    },
                    {
                        "guid": &BookmarkRootGuid::Mobile.as_guid(),
                        "children": [],
                    },
                ],
            }),
        );

        Ok(())
    }
}

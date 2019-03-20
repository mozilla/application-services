/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::create_synced_bookmark_roots;
use super::incoming::IncomingApplicator;
use super::record::{
    guid_to_id, id_to_guid, BookmarkItemRecord, BookmarkRecord, FolderRecord, QueryRecord,
    SeparatorRecord,
};
use super::{SyncedBookmarkKind, SyncedBookmarkValidity};
use crate::api::places_api::ConnectionType;
use crate::db::PlacesDb;
use crate::error::*;
use crate::storage::{bookmarks::BookmarkRootGuid, get_meta, put_meta};
use crate::types::{BookmarkType, SyncGuid, SyncStatus, Timestamp};
use dogear::{
    self, Content, Deletion, IntoTree, Item, LogLevel, MergedDescendant, Tree, UploadReason,
};
use log::{Level, LevelFilter};
use rusqlite::{Row, NO_PARAMS};
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
static LAST_SYNC_META_KEY: &'static str = "bookmarks_last_sync_time";

pub struct BookmarksStore<'a> {
    pub db: &'a PlacesDb,
    pub client_info: &'a Cell<Option<ClientInfo>>,
}

impl<'a> BookmarksStore<'a> {
    pub fn new(db: &'a PlacesDb, client_info: &'a Cell<Option<ClientInfo>>) -> Self {
        assert_eq!(db.conn_type(), ConnectionType::Sync);
        Self { db, client_info }
    }

    fn stage_incoming(
        &self,
        inbound: IncomingChangeset,
        incoming_telemetry: &mut telemetry::EngineIncoming,
    ) -> Result<ServerTimestamp> {
        let timestamp = inbound.timestamp;
        let mut tx = self
            .db
            .time_chunked_transaction(Duration::from_millis(1000))?;

        let applicator = IncomingApplicator::new(&self.db);

        for incoming in inbound.changes {
            applicator.apply_payload(incoming.0, incoming.1)?;
            incoming_telemetry.applied(1);
            tx.maybe_commit()?;
        }
        tx.commit()?;
        Ok(timestamp)
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
            LocalItemsFragment("localItems")
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
                                      parentTitle, dateAdded, title, placeId,
                                      kind, url, keyword, position,
                                      tagFolderName)
            SELECT s.id, s.guid, s.syncChangeCounter, s.parentGuid,
                   s.parentTitle, s.dateAdded, s.title, s.placeId,
                   {kind}, h.url, NULL AS keyword, s.position,
                   NULL AS tagFolderName
            FROM localItems s
            LEFT JOIN moz_places h ON h.id = s.placeId
            LEFT JOIN idsToWeaklyUpload w ON w.id = s.id
            WHERE s.guid <> '{root_guid}' AND (
                    s.syncChangeCounter >= 1 OR
                    w.id NOT NULL
                  )",
            local_items_fragment = LocalItemsFragment("localItems"),
            kind = type_to_kind("s.type", UrlOrPlaceId::Url("h.url")),
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref(),
        ))?;

        // Record the child GUIDs of locally changed folders, which we use to
        // populate the `children` array in the record.
        self.db.execute_batch(
            "
            INSERT INTO structureToUpload(guid, parentId, position)
            SELECT b.guid, b.parent, b.position FROM moz_bookmarks b
            JOIN itemsToUpload o ON o.id = b.parent",
        )?;

        // Stage tags for outgoing bookmarks.
        self.db.execute_batch(
            "
            INSERT INTO tagsToUpload(id, tag)
            SELECT o.id, t.tag
            FROM itemsToUpload o
            JOIN moz_tags_relation r ON r.place_id = o.placeId
            JOIN moz_tags t ON t.id = r.tag_id",
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
        let mut tags_by_local_id: HashMap<i64, Vec<String>> = HashMap::new();

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

        let mut stmt = self.db.prepare("SELECT id, tag FROM tagsToUpload")?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(result) = results.next() {
            let row = result?;
            let local_id = row.get_checked::<_, i64>("id")?;
            let tag = row.get_checked::<_, String>("tag")?;
            let tags = tags_by_local_id.entry(local_id).or_default();
            tags.push(tag);
        }

        let mut stmt = self.db.prepare(
            r#"SELECT id, syncChangeCounter, guid, isDeleted, kind,
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
                outgoing
                    .changes
                    .push(Payload::new_tombstone(guid_to_id(&guid).into()));
                continue;
            }
            let parent_guid = row.get_checked::<_, SyncGuid>("parentGuid")?;
            let parent_title = row.get_checked::<_, String>("parentTitle")?;
            let date_added = row.get_checked::<_, i64>("dateAdded")?;
            let record: BookmarkItemRecord =
                match SyncedBookmarkKind::from_u8(row.get_checked("kind")?)? {
                    SyncedBookmarkKind::Bookmark => {
                        let local_id = row.get_checked::<_, i64>("id")?;
                        let title = row.get_checked::<_, String>("title")?;
                        let url = row.get_checked::<_, String>("url")?;
                        BookmarkRecord {
                            guid,
                            parent_guid: Some(parent_guid),
                            has_dupe: true,
                            parent_title: Some(parent_title),
                            date_added: Some(date_added),
                            title: Some(title),
                            url: Some(url),
                            keyword: None,
                            tags: tags_by_local_id.remove(&local_id).unwrap_or_default(),
                        }
                        .into()
                    }
                    SyncedBookmarkKind::Query => {
                        let title = row.get_checked::<_, String>("title")?;
                        let url = row.get_checked::<_, String>("url")?;
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
                    }
                    SyncedBookmarkKind::Folder => {
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
                    SyncedBookmarkKind::Livemark => continue,
                    SyncedBookmarkKind::Separator => {
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

    /// Decrements the change counter, updates the sync status, and cleans up
    /// tombstones for successfully synced items. Sync calls this method at the
    /// end of each bookmark sync.
    fn push_synced_items(&self, uploaded_at: ServerTimestamp, record_ids: &[String]) -> Result<()> {
        // Flag all successfully synced records as uploaded. This `UPDATE` fires
        // the `pushUploadedChanges` trigger, which updates local change
        // counters and writes the items back to the synced bookmarks table.
        sql_support::each_chunk_mapped(
            &record_ids,
            |id| id_to_guid(id.clone()),
            |chunk, _| -> Result<()> {
                self.db.execute(
                    &format!(
                        "UPDATE itemsToUpload SET
                       uploadedAt = {uploaded_at}
                     WHERE guid IN ({values})",
                        uploaded_at = uploaded_at.as_millis(),
                        values = sql_support::repeat_sql_values(chunk.len())
                    ),
                    chunk,
                )?;
                Ok(())
            },
        )?;

        // Fast-forward the last sync time, so that we don't download the
        // records we just uploaded on the next sync.
        put_meta(
            self.db,
            LAST_SYNC_META_KEY,
            &(uploaded_at.as_millis() as i64),
        )?;

        // Clean up.
        self.db.execute_batch("DELETE FROM itemsToUpload")?;

        Ok(())
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
        let timestamp = self.stage_incoming(inbound, incoming_telemetry)?;

        // write the timestamp now, so if we are interrupted merging or
        // creating outgoing changesets we don't need to re-download the same
        // records.
        put_meta(self.db, LAST_SYNC_META_KEY, &(timestamp.as_millis() as i64))?;

        // Merge and stage outgoing items.
        let merger = Merger::new(&self, timestamp);
        merger.merge()?;

        let outgoing = self.fetch_outgoing_records(timestamp)?;
        Ok(outgoing)
    }

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: &[String],
    ) -> result::Result<(), failure::Error> {
        let tx = self.db.unchecked_transaction()?;
        let result = self.push_synced_items(new_timestamp, records_synced);
        match result {
            Ok(_) => tx.commit()?,
            Err(_) => tx.rollback()?,
        }
        result?;
        Ok(())
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
        let tx = self.db.unchecked_transaction()?;
        self.db.execute_batch(&format!(
            "
                DELETE FROM moz_bookmarks_synced;

                DELETE FROM moz_bookmarks_deleted;

                UPDATE moz_bookmarks
                    SET syncChangeCounter = 0,
                    syncStatus = {}",
            (SyncStatus::New as u8)
        ))?;
        create_synced_bookmark_roots(self.db)?;
        put_meta(self.db, LAST_SYNC_META_KEY, &0)?;
        tx.commit()?;
        Ok(())
    }

    // There's a bit of confusion around 'wipe' in this trait.
    // Logins has `wipe` and `wipe_local`, where the former just wipes the
    // mirror. There's no `wipe_server` (which in theory can be generically
    // implemented for any engine.
    fn wipe(&self) -> result::Result<(), failure::Error> {
        log::warn!("not implemented");
        Ok(())

        /* A wipe_local for bookmarks could probably look something like:
            let tx = self.db.unchecked_transaction()?;
            self.db.conn().execute_cached(
                "
                DELETE from moz_bookmarks;
                DELETE from moz_bookmarks_deleted;
                DELETE from moz_bookmarks_synced;",
                NO_PARAMS,
            )?;
            create_bookmark_roots(self.db)?;
            create_synced_bookmark_roots(self.db)?;
            tx.commit()?;
            Ok(())
        */
    }
}

// We should consider if we can merge log's levels with dogear's.
fn level_filter_to_dogear_level(level: LevelFilter) -> LogLevel {
    match level {
        LevelFilter::Off => LogLevel::Silent,
        LevelFilter::Error => LogLevel::Error,
        LevelFilter::Warn => LogLevel::Warn,
        LevelFilter::Info => LogLevel::Debug, // ???
        LevelFilter::Debug => LogLevel::Debug,
        LevelFilter::Trace => LogLevel::Trace,
    }
}

fn dogear_level_to_level(level: LogLevel) -> Level {
    match level {
        LogLevel::Error => Level::Error,
        LogLevel::Warn => Level::Warn,
        LogLevel::Debug => Level::Info,
        LogLevel::Trace => Level::Trace,
        LogLevel::All => Level::Trace, // ??
        // It doesn't really matter what we map Silent to as we will not be
        // called in that case.
        LogLevel::Silent => Level::Error,
    }
}

struct Driver;

impl dogear::Driver for Driver {
    fn generate_new_guid(&self, _invalid_guid: &dogear::Guid) -> dogear::Result<dogear::Guid> {
        Ok(SyncGuid::new().into())
    }

    fn log_level(&self) -> LogLevel {
        level_filter_to_dogear_level(log::max_level())
    }

    fn log(&self, level: LogLevel, args: fmt::Arguments) {
        log::log!(dogear_level_to_level(level), "{}", args);
    }
}

// The "merger", which is just a thin wrapper for dogear.
struct Merger<'a> {
    store: &'a BookmarksStore<'a>,
    remote_time: ServerTimestamp,
    local_time: Timestamp,
}

impl<'a> Merger<'a> {
    fn new(store: &'a BookmarksStore, remote_time: ServerTimestamp) -> Self {
        Self {
            store,
            remote_time,
            local_time: Timestamp::now(),
        }
    }

    fn merge(&self) -> Result<()> {
        use dogear::Store;
        // Merge and stage outgoing items via dogear.
        self.merge_with_driver(&Driver)?;
        // note we are dropping the result of type dogear::store::Stats here.
        Ok(())
    }

    /// Creates a local tree item from a row in `localItems` or `localRoot`.
    fn local_row_to_item(&self, row: &Row) -> Result<Item> {
        let guid = row.get_checked::<_, SyncGuid>("guid")?;
        let kind = SyncedBookmarkKind::from_u8(row.get_checked("kind")?)?;
        let mut item = Item::new(guid.into(), kind.into());
        // Note that this doesn't account for local clock skew.
        let age = self
            .local_time
            .duration_since(
                row.get_checked::<_, Timestamp>("localModified")
                    .unwrap_or_default(),
            )
            .unwrap_or_default();
        item.age = age.as_secs() as i64 * 1000 + i64::from(age.subsec_millis());
        item.needs_merge = row.get_checked::<_, u32>("syncChangeCounter")? > 0;
        Ok(item)
    }

    /// Creates a remote tree item from a row in `moz_bookmarks_synced`.
    fn remote_row_to_item(&self, row: &Row) -> Result<Item> {
        let guid = row.get_checked::<_, SyncGuid>("guid")?;
        let kind = SyncedBookmarkKind::from_u8(row.get_checked("kind")?)?;
        let mut item = Item::new(guid.into(), kind.into());
        // note that serverModified in this table is an int with ms, which isn't
        // the format of a ServerTimestamp - so we convert it into a number
        // of seconds before creating a ServerTimestamp and doing duration_since.
        let age = self
            .remote_time
            .duration_since(ServerTimestamp(
                row.get_checked::<_, i64>("serverModified").unwrap_or(0) as f64 / 1000.0,
            ))
            .unwrap_or_default();
        item.age = age.as_secs() as i64 * 1000 + i64::from(age.subsec_millis());
        item.needs_merge = row.get_checked("needsMerge")?;
        item.validity = SyncedBookmarkValidity::from_u8(row.get_checked("validity")?)?.into();
        Ok(item)
    }
}

impl<'a> dogear::Store<Error> for Merger<'a> {
    /// Builds a fully rooted, consistent tree from all local items and
    /// tombstones.
    fn fetch_local_tree(&self) -> Result<Tree> {
        let sql = format!(
            "
            WITH RECURSIVE
            {local_items_fragment}
            SELECT s.id, s.guid, s.parentGuid, {kind} AS kind,
                   s.lastModified as localModified, s.syncChangeCounter
            FROM localItems s
            ORDER BY s.level, s.parentId, s.position",
            local_items_fragment = LocalItemsFragment("localItems"),
            kind = type_to_kind("s.type", UrlOrPlaceId::PlaceId("s.placeId")),
        );
        let mut stmt = self.store.db.prepare(&sql)?;
        let mut results = stmt.query(NO_PARAMS)?;
        let mut builder = match results.next() {
            Some(result) => {
                // The first row is always the root.
                let row = result?;
                Tree::with_root(self.local_row_to_item(&row)?)
            }
            None => return Err(ErrorKind::Corruption(Corruption::InvalidLocalRoots).into()),
        };
        while let Some(result) = results.next() {
            // All subsequent rows are descendants.
            let row = result?;
            let parent_guid = row.get_checked::<_, SyncGuid>("parentGuid")?;
            builder
                .item(self.local_row_to_item(&row)?)?
                .by_structure(&parent_guid.into())?;
        }

        let mut tree = builder.into_tree()?;

        // Note tombstones for locally deleted items.
        let mut stmt = self
            .store
            .db
            .prepare("SELECT guid FROM moz_bookmarks_deleted")?;
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
        let mut stmt = self.store.db.prepare(&sql)?;
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
        // Unlike the local tree, items and structure are stored separately, so
        // we use three separate statements to fetch the root, its descendants,
        // and their structure.
        let sql = format!(
            "
            SELECT guid, parentGuid, serverModified, kind, needsMerge, validity
            FROM moz_bookmarks_synced
            WHERE NOT isDeleted AND
                  guid = '{root_guid}'",
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref()
        );
        let mut builder = self
            .store
            .db
            .try_query_row(
                &sql,
                &[],
                |row| -> Result<_> {
                    let root = self.remote_row_to_item(row)?;
                    Ok(Tree::with_root(root))
                },
                false,
            )?
            .ok_or_else(|| ErrorKind::Corruption(Corruption::InvalidSyncedRoots))?;

        let sql = format!(
            "
            SELECT guid, parentGuid, serverModified, kind, needsMerge, validity
            FROM moz_bookmarks_synced
            WHERE NOT isDeleted AND
                  guid <> '{root_guid}'",
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref()
        );
        let mut stmt = self.store.db.prepare(&sql)?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(result) = results.next() {
            let row = result?;
            let p = builder.item(self.remote_row_to_item(&row)?)?;
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
        let mut stmt = self.store.db.prepare(&sql)?;
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
            .store
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
        let mut stmt = self.store.db.prepare(&sql)?;
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
        if !self.store.has_changes()? {
            return Ok(());
        }
        let tx = self.store.db.unchecked_transaction()?;
        let result = self
            .store
            .update_local_items(descendants, deletions)
            .and_then(|_| self.store.stage_local_items_to_upload())
            .and_then(|_| {
                self.store.db.execute_batch(
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

/// A helper that interpolates a named SQL common table expression (CTE) for
/// local items. The CTE may be included in a `WITH RECURSIVE` clause.
struct LocalItemsFragment<'a>(&'a str);

impl<'a> fmt::Display for LocalItemsFragment<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{name}(id, guid, parentId, parentGuid, position, type, title, parentTitle,
                    placeId, dateAdded, lastModified, syncChangeCounter, level) AS (
             SELECT b.id, b.guid, 0, NULL, b.position, b.type, b.title, NULL,
                    b.fk, b.dateAdded, b.lastModified, b.syncChangeCounter, 0
             FROM moz_bookmarks b
             WHERE b.guid = '{root_guid}'
             UNION ALL
             SELECT b.id, b.guid, s.id, s.guid, b.position, b.type, b.title, s.title,
                    b.fk, b.dateAdded, b.lastModified, b.syncChangeCounter, s.level + 1
             FROM moz_bookmarks b
             JOIN {name} s ON s.id = b.parent)",
            name = self.0,
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref()
        )
    }
}

fn type_to_kind<'a>(typ: &'a str, url: UrlOrPlaceId<'a>) -> TypeToKind<'a> {
    TypeToKind { typ, url }
}

/// A helper that interpolates a SQL expression for converting Places item types
/// to Sync record kinds. `typ` is the name of the bookmark type column in the
/// projection.
struct TypeToKind<'a> {
    typ: &'a str,
    url: UrlOrPlaceId<'a>,
}

impl<'a> fmt::Display for TypeToKind<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            r#"(CASE {typ}
                WHEN {bookmark_type} THEN (
                    CASE substr({url}, 1, 6)
                    /* Queries are bookmarks with a "place:" URL scheme. */
                    WHEN 'place:' THEN {query_kind}
                    ELSE {bookmark_kind}
                    END
                )
                WHEN {folder_type} THEN {folder_kind}
                ELSE {separator_kind}
                END)"#,
            typ = self.typ,
            bookmark_type = BookmarkType::Bookmark as u8,
            url = self.url,
            bookmark_kind = SyncedBookmarkKind::Bookmark as u8,
            folder_type = BookmarkType::Folder as u8,
            folder_kind = SyncedBookmarkKind::Folder as u8,
            separator_kind = SyncedBookmarkKind::Separator as u8,
            query_kind = SyncedBookmarkKind::Query as u8
        )
    }
}

/// A helper that interpolates a SQL expression for a Place URL. This avoids a
/// subquery if the URL is already available in the projection.
enum UrlOrPlaceId<'a> {
    Url(&'a str),
    PlaceId(&'a str),
}

impl<'a> fmt::Display for UrlOrPlaceId<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            UrlOrPlaceId::Url(s) => write!(f, "{}", s),
            UrlOrPlaceId::PlaceId(s) => {
                write!(f, "(SELECT h.url FROM moz_places h WHERE h.id = {})", s)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::{test::new_mem_api, ConnectionType};
    use crate::bookmark_sync::store::BookmarksStore;
    use crate::db::PlacesDb;
    use crate::storage::{bookmarks::get_raw_bookmark, tags};
    use crate::tests::{
        assert_json_tree as assert_local_json_tree, insert_json_tree as insert_local_json_tree,
    };
    use dogear::{Store as DogearStore, Validity};
    use pretty_assertions::assert_eq;
    use serde_json::{json, Value};
    use url::Url;

    use std::cell::Cell;
    use sync15::Payload;

    fn apply_incoming(records_json: Value) -> PlacesDb {
        let api = new_mem_api();
        let conn = api
            .open_connection(ConnectionType::Sync)
            .expect("should get a connection");

        // suck records into the store.
        let client_info = Cell::new(None);
        let store = BookmarksStore::new(&conn, &client_info);

        let mut incoming =
            IncomingChangeset::new(store.collection_name().to_string(), ServerTimestamp(0.0));

        match records_json {
            Value::Array(records) => {
                for record in records {
                    let payload = Payload::from_json(record).unwrap();
                    incoming.changes.push((payload, ServerTimestamp(0.0)));
                }
            }
            Value::Object(_) => {
                let payload = Payload::from_json(records_json).unwrap();
                incoming.changes.push((payload, ServerTimestamp(0.0)));
            }
            _ => panic!("unexpected json value"),
        }

        store
            .apply_incoming(incoming, &mut telemetry::EngineIncoming::new())
            .expect("Should apply incoming and stage outgoing records");
        conn
    }

    fn assert_incoming_creates_local_tree(
        records_json: Value,
        local_folder: &SyncGuid,
        local_tree: Value,
    ) {
        let conn = apply_incoming(records_json);
        assert_local_json_tree(&conn, local_folder, local_tree);
    }

    #[test]
    fn test_fetch_remote_tree() -> Result<()> {
        let _ = env_logger::try_init();
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
        let client_info = Cell::new(None);
        let store = BookmarksStore::new(&conn, &client_info);

        let mut incoming =
            IncomingChangeset::new(store.collection_name().to_string(), ServerTimestamp(0.0));

        for record in records {
            let payload = Payload::from_json(record).unwrap();
            incoming.changes.push((payload, ServerTimestamp(0.0)));
        }

        store
            .stage_incoming(incoming, &mut telemetry::EngineIncoming::new())
            .expect("Should apply incoming and stage outgoing records");

        let merger = Merger::new(&store, ServerTimestamp(0.0));

        let tree = merger.fetch_remote_tree()?;

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

        let client_info = Cell::new(None);
        let store = BookmarksStore::new(&conn, &client_info);
        let merger = Merger::new(&store, ServerTimestamp(0.0));

        let tree = merger.fetch_local_tree()?;

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
    fn test_apply_bookmark() {
        assert_incoming_creates_local_tree(
            json!([{
                // A valid query (which actually looks just like a bookmark, but that's ok)
                "id": "bookmark1___",
                "type": "bookmark",
                "parentid": "unfiled",
                "parentName": "Unfiled Bookmarks",
                "dateAdded": 1381542355843u64,
                "title": "Some bookmark",
                "bmkUri": "http://example.com",
            },
            {
                "id": "unfiled",
                "type": "folder",
                "parentid": "root",
                "dateAdded": 1381542355843u64,
                "title": "Unfiled",
                "children": ["bookmark1___"],
            }]),
            &BookmarkRootGuid::Unfiled.as_guid(),
            json!({"children" : [{"guid": "bookmark1___", "url": "http://example.com"}]}),
        );
    }

    #[test]
    fn test_apply_query() {
        // should we add some more query variations here?
        assert_incoming_creates_local_tree(
            json!([{
                "id": "query1______",
                "type": "query",
                "parentid": "unfiled",
                "parentName": "Unfiled Bookmarks",
                "dateAdded": 1381542355843u64,
                "title": "Some query",
                "bmkUri": "place:tag=foo",
            },
            {
                "id": "unfiled",
                "type": "folder",
                "parentid": "root",
                "dateAdded": 1381542355843u64,
                "title": "Unfiled",
                "children": ["query1______"],
            }]),
            &BookmarkRootGuid::Unfiled.as_guid(),
            json!({"children" : [{"guid": "query1______", "url": "place:tag=foo"}]}),
        );
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
        tags::tag_url(
            &conn,
            &Url::parse("http://example.com/a").expect("Should parse URL for A"),
            "baz",
        )
        .expect("Should tag A");

        let records = vec![
            json!({
                "id": "bookmarkCCCC",
                "type": "bookmark",
                "parentid": BookmarkRootGuid::Menu.as_guid(),
                "parentName": "menu",
                "dateAdded": 1552183116885u64,
                "title": "C",
                "bmkUri": "http://example.com/c",
                "tags": ["foo", "bar"],
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

        let client_info = Cell::new(None);
        let store = BookmarksStore::new(&conn, &client_info);

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
            vec!["bookmarkAAAA", "bookmarkBBBB", "unfiled",]
        );
        let record_for_a = outgoing
            .changes
            .iter()
            .find(|p| p.id == "bookmarkAAAA")
            .expect("Should upload A");
        assert_eq!(
            record_for_a.data["tags"]
                .as_array()
                .expect("Should upload tags for A"),
            &["baz"]
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

        // We haven't finished the sync yet, so all local change counts for
        // items to upload should still be > 0.
        let guid_for_a: SyncGuid = "bookmarkAAAA".into();
        let info_for_a = get_raw_bookmark(&conn, &guid_for_a)
            .expect("Should fetch info for A")
            .unwrap();
        assert_eq!(info_for_a.sync_change_counter, 1);
        let info_for_unfiled = get_raw_bookmark(&conn, &BookmarkRootGuid::Unfiled.as_guid())
            .expect("Should fetch info for unfiled")
            .unwrap();
        assert_eq!(info_for_unfiled.sync_change_counter, 1);

        store
            .sync_finished(
                ServerTimestamp(0.0),
                &[
                    "bookmarkAAAA".into(),
                    "bookmarkBBBB".into(),
                    "unfiled".into(),
                ],
            )
            .expect("Should push synced changes back to the store");

        let info_for_a = get_raw_bookmark(&conn, &guid_for_a)
            .expect("Should fetch info for A")
            .unwrap();
        assert_eq!(info_for_a.sync_change_counter, 0);
        let info_for_unfiled = get_raw_bookmark(&conn, &BookmarkRootGuid::Unfiled.as_guid())
            .expect("Should fetch info for unfiled")
            .unwrap();
        assert_eq!(info_for_unfiled.sync_change_counter, 0);

        let mut tags_for_c = tags::get_tags_for_url(
            &conn,
            &Url::parse("http://example.com/c").expect("Should parse URL for C"),
        )
        .expect("Should return tags for C");
        tags_for_c.sort();
        assert_eq!(tags_for_c, &["bar", "foo"]);

        Ok(())
    }
}

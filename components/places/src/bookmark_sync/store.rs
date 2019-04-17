/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::create_synced_bookmark_roots;
use super::incoming::IncomingApplicator;
use super::record::{
    BookmarkItemRecord, BookmarkRecord, BookmarkRecordId, FolderRecord, QueryRecord,
    SeparatorRecord,
};
use super::{SyncedBookmarkKind, SyncedBookmarkValidity};
use crate::api::places_api::ConnectionType;
use crate::db::{PlacesDb, PlacesTransaction};
use crate::error::*;
use crate::storage::{bookmarks::BookmarkRootGuid, delete_meta, get_meta, put_meta};
use crate::types::{BookmarkType, SyncGuid, SyncStatus, Timestamp};
use dogear::{
    self, Content, Deletion, IntoTree, Item, MergedDescendant, MergedRoot, Tree, UploadReason,
};
use rusqlite::{Row, NO_PARAMS};
use sql_support::{self, ConnExt, SqlInterruptScope};
use std::collections::HashMap;
use std::fmt;
use std::result;
use sync15::{
    telemetry, CollSyncIds, CollectionRequest, IncomingChangeset, KeyBundle, MemoryCachedState,
    OutgoingChangeset, Payload, ServerTimestamp, Store, StoreSyncAssociation,
    Sync15StorageClientInit,
};
pub const LAST_SYNC_META_KEY: &str = "bookmarks_last_sync_time";
// Note that all engines in this crate should use a *different* meta key
// for the global sync ID, because engines are reset individually.
const GLOBAL_SYNCID_META_KEY: &str = "bookmarks_global_sync_id";
const COLLECTION_SYNCID_META_KEY: &str = "bookmarks_sync_id";

pub struct BookmarksStore<'a> {
    pub db: &'a PlacesDb,
    interruptee: &'a SqlInterruptScope,
}

impl<'a> BookmarksStore<'a> {
    pub fn new(db: &'a PlacesDb, interruptee: &'a SqlInterruptScope) -> Self {
        assert_eq!(db.conn_type(), ConnectionType::Sync);
        Self { db, interruptee }
    }

    fn stage_incoming(
        &self,
        inbound: IncomingChangeset,
        incoming_telemetry: &mut telemetry::EngineIncoming,
    ) -> Result<ServerTimestamp> {
        let timestamp = inbound.timestamp;
        let mut tx = self.db.begin_transaction()?;

        let applicator = IncomingApplicator::new(&self.db);

        for incoming in inbound.changes {
            applicator.apply_payload(incoming.0, incoming.1)?;
            incoming_telemetry.applied(1);
            tx.maybe_commit()?;
            self.interruptee.err_if_interrupted()?;
        }
        tx.commit()?;
        Ok(timestamp)
    }

    fn has_changes(&self) -> Result<bool> {
        // In the first subquery, we check incoming items with needsMerge = true
        // except the tombstones who don't correspond to any local bookmark because
        // we don't store them yet, hence never "merged" (see bug 1343103).
        let sql = format!(
            "SELECT
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
             AS hasChanges",
            LocalItemsFragment("localItems")
        );
        Ok(self
            .db
            .try_query_row(
                &sql,
                &[],
                |row| -> rusqlite::Result<_> { Ok(row.get::<_, bool>(0)?) },
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
        deletions: Vec<Deletion<'_>>,
        _tx: &mut PlacesTransaction<'_>,
    ) -> Result<()> {
        // First, insert rows for all merged descendants.
        sql_support::each_sized_chunk(
            &descendants,
            sql_support::default_max_variable_number() / 4,
            |chunk, _| -> Result<()> {
                // We can't avoid allocating here, since we're binding four
                // parameters per descendant. Rust's `SliceConcatExt::concat`
                // is semantically equivalent, but requires a second allocation,
                // which we _can_ avoid by writing this out.
                self.interruptee.err_if_interrupted()?;
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
                self.db.execute(&format!("
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

        // Next, insert rows for deletions.
        sql_support::each_chunk(&deletions, |chunk, _| -> Result<()> {
            self.db.execute(
                &format!(
                    "INSERT INTO itemsToRemove(guid, localLevel, shouldUploadTombstone)
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

        // `itemsToMerge` is a view, so "deleting" from it fires the
        // `insertNewLocalItems` and `updateExistingLocalItems`
        // triggers instead.
        self.db.execute_batch("DELETE FROM itemsToMerge")?;

        // `structureToMerge` is also a view, so "deleting" from it fires the
        // `updateLocalStructure` trigger.
        self.db.execute_batch("DELETE FROM structureToMerge")?;

        // Deleting from `itemsToRemove` fires the `removeLocalItems` trigger.
        self.db.execute_batch("DELETE FROM itemsToRemove")?;

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
            "INSERT OR IGNORE INTO idsToWeaklyUpload(id)
             SELECT b.id FROM moz_bookmarks b
             JOIN mergedTree r ON r.mergedGuid = b.guid
             JOIN moz_bookmarks_synced v ON v.guid = r.remoteGuid
             WHERE r.useRemote AND
                  b.dateAdded < v.dateAdded",
        )?;

        // Stage remaining locally changed items for upload.
        self.db.execute_batch(&format!(
            "WITH RECURSIVE
             {local_items_fragment}
             INSERT INTO itemsToUpload(id, guid, syncChangeCounter, parentGuid,
                                       parentTitle, dateAdded, title, placeId,
                                       kind, url, keyword, position)
             SELECT s.id, s.guid, s.syncChangeCounter, s.parentGuid,
                    s.parentTitle, s.dateAdded, s.title, s.placeId,
                    {kind}, h.url, v.keyword, s.position
             FROM localItems s
             JOIN mergedTree r ON r.mergedGuid = s.guid
             LEFT JOIN moz_bookmarks_synced v ON v.guid = r.remoteGuid
             LEFT JOIN moz_places h ON h.id = s.placeId
             LEFT JOIN idsToWeaklyUpload w ON w.id = s.id
             WHERE s.guid <> '{root_guid}' AND
                   (s.syncChangeCounter > 0 OR w.id NOT NULL)",
            local_items_fragment = LocalItemsFragment("localItems"),
            kind = item_kind_fragment("s.type", UrlOrPlaceIdFragment::Url("h.url")),
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref(),
        ))?;

        // Record the child GUIDs of locally changed folders, which we use to
        // populate the `children` array in the record.
        self.db.execute_batch(
            "INSERT INTO structureToUpload(guid, parentId, position)
             SELECT b.guid, b.parent, b.position FROM moz_bookmarks b
             JOIN itemsToUpload o ON o.id = b.parent",
        )?;

        // Stage tags for outgoing bookmarks.
        self.db.execute_batch(
            "INSERT INTO tagsToUpload(id, tag)
             SELECT o.id, t.tag
             FROM itemsToUpload o
             JOIN moz_tags_relation r ON r.place_id = o.placeId
             JOIN moz_tags t ON t.id = r.tag_id",
        )?;

        // Finally, stage tombstones for deleted items.
        self.db.execute_batch(
            "INSERT OR IGNORE INTO itemsToUpload(guid, syncChangeCounter, isDeleted)
             SELECT guid, 1, 1 FROM moz_bookmarks_deleted",
        )?;

        Ok(())
    }

    /// Inflates Sync records for all staged outgoing items.
    fn fetch_outgoing_records(&self, timestamp: ServerTimestamp) -> Result<OutgoingChangeset> {
        let mut outgoing = OutgoingChangeset::new(self.collection_name().into(), timestamp);
        let mut child_record_ids_by_local_parent_id: HashMap<i64, Vec<BookmarkRecordId>> =
            HashMap::new();
        let mut tags_by_local_id: HashMap<i64, Vec<String>> = HashMap::new();

        let mut stmt = self.db.prepare(
            "SELECT parentId, guid FROM structureToUpload
             ORDER BY parentId, position",
        )?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(row) = results.next()? {
            self.interruptee.err_if_interrupted()?;
            let local_parent_id = row.get::<_, i64>("parentId")?;
            let child_guid = row.get::<_, SyncGuid>("guid")?;
            let child_record_ids = child_record_ids_by_local_parent_id
                .entry(local_parent_id)
                .or_default();
            child_record_ids.push(child_guid.into());
        }

        let mut stmt = self.db.prepare("SELECT id, tag FROM tagsToUpload")?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(row) = results.next()? {
            self.interruptee.err_if_interrupted()?;
            let local_id = row.get::<_, i64>("id")?;
            let tag = row.get::<_, String>("tag")?;
            let tags = tags_by_local_id.entry(local_id).or_default();
            tags.push(tag);
        }

        let mut stmt = self.db.prepare(
            r#"SELECT id, syncChangeCounter, guid, isDeleted, kind, keyword,
                      url, IFNULL(title, "") AS title, position, parentGuid,
                      IFNULL(parentTitle, "") AS parentTitle, dateAdded
               FROM itemsToUpload"#,
        )?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(row) = results.next()? {
            self.interruptee.err_if_interrupted()?;
            let guid = row.get::<_, SyncGuid>("guid")?;
            let is_deleted = row.get::<_, bool>("isDeleted")?;
            if is_deleted {
                outgoing.changes.push(Payload::new_tombstone(
                    BookmarkRecordId::from(guid).into_payload_id(),
                ));
                continue;
            }
            let parent_guid = row.get::<_, SyncGuid>("parentGuid")?;
            let parent_title = row.get::<_, String>("parentTitle")?;
            let date_added = row.get::<_, i64>("dateAdded")?;
            let record: BookmarkItemRecord = match SyncedBookmarkKind::from_u8(row.get("kind")?)? {
                SyncedBookmarkKind::Bookmark => {
                    let local_id = row.get::<_, i64>("id")?;
                    let title = row.get::<_, String>("title")?;
                    let url = row.get::<_, String>("url")?;
                    BookmarkRecord {
                        record_id: guid.into(),
                        parent_record_id: Some(parent_guid.into()),
                        parent_title: Some(parent_title),
                        date_added: Some(date_added),
                        has_dupe: true,
                        title: Some(title),
                        url: Some(url),
                        keyword: row.get::<_, Option<String>>("keyword")?,
                        tags: tags_by_local_id.remove(&local_id).unwrap_or_default(),
                    }
                    .into()
                }
                SyncedBookmarkKind::Query => {
                    let title = row.get::<_, String>("title")?;
                    let url = row.get::<_, String>("url")?;
                    QueryRecord {
                        record_id: guid.into(),
                        parent_record_id: Some(parent_guid.into()),
                        parent_title: Some(parent_title),
                        date_added: Some(date_added),
                        has_dupe: true,
                        title: Some(title),
                        url: Some(url),
                        tag_folder_name: None,
                    }
                    .into()
                }
                SyncedBookmarkKind::Folder => {
                    let title = row.get::<_, String>("title")?;
                    let local_id = row.get::<_, i64>("id")?;
                    let children = child_record_ids_by_local_parent_id
                        .remove(&local_id)
                        .unwrap_or_default();
                    FolderRecord {
                        record_id: guid.into(),
                        parent_record_id: Some(parent_guid.into()),
                        parent_title: Some(parent_title),
                        date_added: Some(date_added),
                        has_dupe: true,
                        title: Some(title),
                        children,
                    }
                    .into()
                }
                SyncedBookmarkKind::Livemark => continue,
                SyncedBookmarkKind::Separator => {
                    let position = row.get::<_, i64>("position")?;
                    SeparatorRecord {
                        record_id: guid.into(),
                        parent_record_id: Some(parent_guid.into()),
                        parent_title: Some(parent_title),
                        date_added: Some(date_added),
                        has_dupe: true,
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
    fn push_synced_items(
        &self,
        uploaded_at: ServerTimestamp,
        records_synced: Vec<String>,
    ) -> Result<()> {
        // Flag all successfully synced records as uploaded. This `UPDATE` fires
        // the `pushUploadedChanges` trigger, which updates local change
        // counters and writes the items back to the synced bookmarks table.
        let mut tx = self.db.begin_transaction()?;

        let guids = records_synced
            .into_iter()
            .map(|id| BookmarkRecordId::from_payload_id(id).into())
            .collect::<Vec<SyncGuid>>();
        sql_support::each_chunk(&guids, |chunk, _| -> Result<()> {
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
            tx.maybe_commit()?;
            self.interruptee.err_if_interrupted()?;
            Ok(())
        })?;

        // Fast-forward the last sync time, so that we don't download the
        // records we just uploaded on the next sync.
        put_meta(
            self.db,
            LAST_SYNC_META_KEY,
            &(uploaded_at.as_millis() as i64),
        )?;

        // Clean up.
        self.db.execute_batch("DELETE FROM itemsToUpload")?;
        tx.commit()?;

        Ok(())
    }

    pub fn sync(
        &self,
        storage_init: &Sync15StorageClientInit,
        root_sync_key: &KeyBundle,
        mem_cached_state: &mut MemoryCachedState,
        disk_cached_state: &mut Option<String>,
        sync_ping: &mut telemetry::SyncTelemetryPing,
    ) -> Result<()> {
        let result = sync15::sync_multiple(
            &[self],
            disk_cached_state,
            mem_cached_state,
            storage_init,
            root_sync_key,
            sync_ping,
            self.interruptee,
        );
        let failures = result?;
        if failures.is_empty() {
            Ok(())
        } else {
            let (_, err) = failures.into_iter().next().unwrap();
            Err(err.into())
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
        let timestamp = self.stage_incoming(inbound, incoming_telemetry)?;

        // write the timestamp now, so if we are interrupted merging or
        // creating outgoing changesets we don't need to re-download the same
        // records.
        put_meta(self.db, LAST_SYNC_META_KEY, &(timestamp.as_millis() as i64))?;

        // Merge and stage outgoing items.
        let mut merger = Merger::new(&self, timestamp);
        merger.merge()?;

        let outgoing = self.fetch_outgoing_records(timestamp)?;
        Ok(outgoing)
    }

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: Vec<String>,
    ) -> result::Result<(), failure::Error> {
        self.push_synced_items(new_timestamp, records_synced)?;
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

    fn get_sync_assoc(&self) -> result::Result<StoreSyncAssociation, failure::Error> {
        let global = get_meta(self.db, GLOBAL_SYNCID_META_KEY)?;
        let coll = get_meta(self.db, COLLECTION_SYNCID_META_KEY)?;
        Ok(if let (Some(global), Some(coll)) = (global, coll) {
            StoreSyncAssociation::Connected(CollSyncIds { global, coll })
        } else {
            StoreSyncAssociation::Disconnected
        })
    }

    /// Removes all sync metadata, such that the next sync is treated as a
    /// first sync. Unlike `wipe`, this keeps all local items, but clears
    /// all synced items and pending tombstones. This also forgets the last
    /// sync time.
    fn reset(&self, assoc: &StoreSyncAssociation) -> result::Result<(), failure::Error> {
        let tx = self.db.begin_transaction()?;
        self.db.execute_batch(&format!(
            "DELETE FROM moz_bookmarks_synced;

             DELETE FROM moz_bookmarks_deleted;

             UPDATE moz_bookmarks
             SET syncChangeCounter = 0,
                 syncStatus = {}",
            (SyncStatus::New as u8)
        ))?;
        create_synced_bookmark_roots(self.db)?;
        put_meta(self.db, LAST_SYNC_META_KEY, &0)?;
        match assoc {
            StoreSyncAssociation::Disconnected => {
                delete_meta(self.db, GLOBAL_SYNCID_META_KEY)?;
                delete_meta(self.db, COLLECTION_SYNCID_META_KEY)?;
            }
            StoreSyncAssociation::Connected(ids) => {
                put_meta(self.db, GLOBAL_SYNCID_META_KEY, &ids.global)?;
                put_meta(self.db, COLLECTION_SYNCID_META_KEY, &ids.coll)?;
            }
        };
        tx.commit()?;
        Ok(())
    }

    /// Erases all local items. Unlike `reset`, this keeps all synced items
    /// until the next sync, when they will be replaced with tombstones. This
    /// also preserves the last sync time.
    ///
    /// Conceptually, the next sync will merge an empty local tree, and a full
    /// remote tree.
    fn wipe(&self) -> result::Result<(), failure::Error> {
        let tx = self.db.begin_transaction()?;
        let sql = format!(
            "INSERT INTO moz_bookmarks_deleted(guid, dateRemoved)
             SELECT guid, now()
             FROM moz_bookmarks
             WHERE guid NOT IN {roots} AND
                   syncStatus = {sync_status};

             UPDATE moz_bookmarks SET
               syncChangeCounter = syncChangeCounter + 1
             WHERE guid IN {roots};

             DELETE FROM moz_bookmarks
             WHERE guid NOT IN {roots};",
            roots = RootsFragment(&[
                BookmarkRootGuid::Root,
                BookmarkRootGuid::Menu,
                BookmarkRootGuid::Mobile,
                BookmarkRootGuid::Toolbar,
                BookmarkRootGuid::Unfiled
            ]),
            sync_status = SyncStatus::Normal as u8
        );
        self.db.execute_batch(&sql)?;
        create_synced_bookmark_roots(self.db)?;
        tx.commit()?;
        Ok(())
    }
}

struct Driver;

impl dogear::Driver for Driver {
    fn generate_new_guid(&self, _invalid_guid: &dogear::Guid) -> dogear::Result<dogear::Guid> {
        Ok(SyncGuid::new().into())
    }
}

// The "merger", which is just a thin wrapper for dogear.
struct Merger<'a> {
    store: &'a BookmarksStore<'a>,
    remote_time: ServerTimestamp,
    local_time: Timestamp,
}

impl<'a> Merger<'a> {
    fn new(store: &'a BookmarksStore<'_>, remote_time: ServerTimestamp) -> Self {
        Self {
            store,
            remote_time,
            local_time: Timestamp::now(),
        }
    }

    fn merge(&mut self) -> Result<()> {
        use dogear::Store;
        if !self.store.has_changes()? {
            return Ok(());
        }
        // Merge and stage outgoing items via dogear.
        let stats = self.merge_with_driver(&Driver)?;
        log::debug!("merge completed: {:?}", stats);
        Ok(())
    }

    /// Creates a local tree item from a row in the `localItems` CTE.
    fn local_row_to_item(&self, row: &Row<'_>) -> Result<Item> {
        let guid = row.get::<_, SyncGuid>("guid")?;
        let kind = SyncedBookmarkKind::from_u8(row.get("kind")?)?;
        let mut item = Item::new(guid.into(), kind.into());
        // Note that this doesn't account for local clock skew.
        let age = self
            .local_time
            .duration_since(row.get::<_, Timestamp>("localModified")?)
            .unwrap_or_default();
        item.age = age.as_secs() as i64 * 1000 + i64::from(age.subsec_millis());
        item.needs_merge = row.get::<_, u32>("syncChangeCounter")? > 0;
        Ok(item)
    }

    /// Creates a remote tree item from a row in `moz_bookmarks_synced`.
    fn remote_row_to_item(&self, row: &Row<'_>) -> Result<Item> {
        let guid = row.get::<_, SyncGuid>("guid")?;
        let kind = SyncedBookmarkKind::from_u8(row.get("kind")?)?;
        let mut item = Item::new(guid.into(), kind.into());
        // note that serverModified in this table is an int with ms, which isn't
        // the format of a ServerTimestamp - so we convert it into a number
        // of seconds before creating a ServerTimestamp and doing duration_since.
        let age = self
            .remote_time
            .duration_since(ServerTimestamp(
                row.get::<_, f64>("serverModified")? / 1000.0,
            ))
            .unwrap_or_default();
        item.age = age.as_secs() as i64 * 1000 + i64::from(age.subsec_millis());
        item.needs_merge = row.get("needsMerge")?;
        item.validity = SyncedBookmarkValidity::from_u8(row.get("validity")?)?.into();
        Ok(item)
    }
}

impl<'a> dogear::Store<Error> for Merger<'a> {
    /// Builds a fully rooted, consistent tree from all local items and
    /// tombstones.
    fn fetch_local_tree(&self) -> Result<Tree> {
        let sql = format!(
            "WITH RECURSIVE
             {local_items_fragment}
             SELECT s.id, s.guid, s.parentGuid, {kind} AS kind,
                    s.lastModified as localModified, s.syncChangeCounter
             FROM localItems s
             ORDER BY s.level, s.parentId, s.position",
            local_items_fragment = LocalItemsFragment("localItems"),
            kind = item_kind_fragment("s.type", UrlOrPlaceIdFragment::PlaceId("s.placeId")),
        );
        let mut stmt = self.store.db.prepare(&sql)?;
        let mut results = stmt.query(NO_PARAMS)?;
        let mut builder = match results.next()? {
            Some(row) => {
                // The first row is always the root.
                Tree::with_root(self.local_row_to_item(&row)?)
            }
            None => return Err(ErrorKind::Corruption(Corruption::InvalidLocalRoots).into()),
        };
        while let Some(row) = results.next()? {
            // All subsequent rows are descendants.
            let parent_guid = row.get::<_, SyncGuid>("parentGuid")?;
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
        let rows = stmt.query_and_then(NO_PARAMS, |row| row.get::<_, SyncGuid>("guid"))?;
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
            "SELECT b.guid, b.type, IFNULL(b.title, '') AS title, h.url,
                    b.position
             FROM moz_bookmarks b
             JOIN moz_bookmarks p ON p.id = b.parent
             LEFT JOIN moz_places h ON h.id = b.fk
             LEFT JOIN moz_bookmarks_synced v ON v.guid = b.guid
             WHERE v.guid IS NULL AND
                   p.guid <> '{root_guid}' AND
                   b.syncStatus <> {sync_status}",
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref(),
            sync_status = SyncStatus::Normal as u8
        );
        let mut stmt = self.store.db.prepare(&sql)?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(row) = results.next()? {
            let typ = match BookmarkType::from_u8(row.get("type")?) {
                Some(t) => t,
                None => continue,
            };
            let content = match typ {
                BookmarkType::Bookmark => {
                    let title = row.get("title")?;
                    let url_href = row.get("url")?;
                    Content::Bookmark { title, url_href }
                }
                BookmarkType::Folder => {
                    let title = row.get("title")?;
                    Content::Folder { title }
                }
                BookmarkType::Separator => {
                    let position = row.get("position")?;
                    Content::Separator { position }
                }
            };
            let guid = row.get::<_, SyncGuid>("guid")?;
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
            "SELECT guid, serverModified, kind, needsMerge, validity
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
        builder.reparent_orphans_to(&dogear::UNFILED_GUID);

        let sql = format!(
            "SELECT guid, parentGuid, serverModified, kind, needsMerge, validity
             FROM moz_bookmarks_synced
             WHERE NOT isDeleted AND
                   guid <> '{root_guid}'
             ORDER BY guid",
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref()
        );
        let mut stmt = self.store.db.prepare(&sql)?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(row) = results.next()? {
            let p = builder.item(self.remote_row_to_item(&row)?)?;
            if let Some(parent_guid) = row.get::<_, Option<SyncGuid>>("parentGuid")? {
                p.by_parent_guid(parent_guid.into())?;
            }
        }

        let sql = format!(
            "SELECT guid, parentGuid FROM moz_bookmarks_synced_structure
             WHERE guid <> '{root_guid}'
             ORDER BY parentGuid, position",
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref()
        );
        let mut stmt = self.store.db.prepare(&sql)?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(row) = results.next()? {
            let guid = row.get::<_, SyncGuid>("guid")?;
            let parent_guid = row.get::<_, SyncGuid>("parentGuid")?;
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
        let rows = stmt.query_and_then(NO_PARAMS, |row| row.get::<_, SyncGuid>("guid"))?;
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
            "SELECT v.guid, v.kind, IFNULL(v.title, '') AS title, h.url,
                    s.position
             FROM moz_bookmarks_synced v
             JOIN moz_bookmarks_synced_structure s ON s.guid = v.guid
             LEFT JOIN moz_places h ON h.id = v.placeId
             LEFT JOIN moz_bookmarks b ON b.guid = v.guid
             WHERE NOT v.isDeleted AND
                   v.needsMerge AND
                   b.guid IS NULL AND
                   s.parentGuid <> '{root_guid}'",
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref()
        );
        let mut stmt = self.store.db.prepare(&sql)?;
        let mut results = stmt.query(NO_PARAMS)?;
        while let Some(row) = results.next()? {
            let content = match SyncedBookmarkKind::from_u8(row.get("kind")?)? {
                SyncedBookmarkKind::Bookmark | SyncedBookmarkKind::Query => {
                    let title = row.get("title")?;
                    let url_href = row.get("url")?;
                    Content::Bookmark { title, url_href }
                }
                SyncedBookmarkKind::Folder => {
                    let title = row.get("title")?;
                    Content::Folder { title }
                }
                SyncedBookmarkKind::Separator => {
                    let position = row.get("position")?;
                    Content::Separator { position }
                }
                _ => continue,
            };
            let guid = row.get::<_, SyncGuid>("guid")?;
            contents.insert(guid.into(), content);
        }

        Ok(contents)
    }

    fn apply<'t>(
        &mut self,
        root: MergedRoot<'t>,
        deletions: impl Iterator<Item = Deletion<'t>>,
    ) -> Result<()> {
        let descendants = root.descendants();
        let deletions = deletions.collect::<Vec<_>>();

        let mut tx = self.store.db.begin_transaction()?;
        self.store
            .update_local_items(descendants, deletions, &mut tx)?;
        self.store.stage_local_items_to_upload()?;
        self.store.db.execute_batch(
            "DELETE FROM mergedTree;
             DELETE FROM idsToWeaklyUpload;",
        )?;
        tx.commit()?;
        Ok(())
    }
}

/// A helper that interpolates a named SQL common table expression (CTE) for
/// local items. The CTE may be included in a `WITH RECURSIVE` clause.
struct LocalItemsFragment<'a>(&'a str);

impl<'a> fmt::Display for LocalItemsFragment<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

fn item_kind_fragment(
    type_column_name: &'static str,
    url_or_place_id_fragment: UrlOrPlaceIdFragment,
) -> ItemKindFragment {
    ItemKindFragment {
        type_column_name,
        url_or_place_id_fragment,
    }
}

/// A helper that interpolates a SQL expression for converting a local item
/// type to a synced item kind.
struct ItemKindFragment {
    /// The name of the column containing the Places item type.
    type_column_name: &'static str,
    /// The column containing the item's URL or Place ID.
    url_or_place_id_fragment: UrlOrPlaceIdFragment,
}

impl fmt::Display for ItemKindFragment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "(CASE {typ}
              WHEN {bookmark_type} THEN (
                  CASE substr({url}, 1, 6)
                  /* Queries are bookmarks with a 'place:' URL scheme. */
                  WHEN 'place:' THEN {query_kind}
                  ELSE {bookmark_kind}
                  END
              )
              WHEN {folder_type} THEN {folder_kind}
              ELSE {separator_kind}
              END)",
            typ = self.type_column_name,
            bookmark_type = BookmarkType::Bookmark as u8,
            url = self.url_or_place_id_fragment,
            bookmark_kind = SyncedBookmarkKind::Bookmark as u8,
            folder_type = BookmarkType::Folder as u8,
            folder_kind = SyncedBookmarkKind::Folder as u8,
            separator_kind = SyncedBookmarkKind::Separator as u8,
            query_kind = SyncedBookmarkKind::Query as u8
        )
    }
}

/// A helper that interpolates a SQL expression for querying a local item's
/// URL. Note that the `&'static str` for each variant specifies the _name of
/// the column_ containing the URL or ID, not the URL or ID itself.
enum UrlOrPlaceIdFragment {
    /// The name of the column containing the URL. This avoids a subquery if
    /// a column for the URL already exists in the query.
    Url(&'static str),
    /// The name of the column containing the Place ID. This writes out a
    /// subquery to look up the URL.
    PlaceId(&'static str),
}

impl fmt::Display for UrlOrPlaceIdFragment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UrlOrPlaceIdFragment::Url(s) => write!(f, "{}", s),
            UrlOrPlaceIdFragment::PlaceId(s) => {
                write!(f, "(SELECT h.url FROM moz_places h WHERE h.id = {})", s)
            }
        }
    }
}

/// A helper that interpolates a SQL list containing the given bookmark
/// root GUIDs.
struct RootsFragment<'a>(&'a [BookmarkRootGuid]);

impl<'a> fmt::Display for RootsFragment<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("(")?;
        for (i, guid) in self.0.iter().enumerate() {
            if i != 0 {
                f.write_str(",")?;
            }
            write!(f, "'{}'", guid.as_str())?;
        }
        f.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::{test::new_mem_api, ConnectionType, PlacesApi};
    use crate::bookmark_sync::store::BookmarksStore;
    use crate::db::PlacesDb;
    use crate::storage::{
        bookmarks::{get_raw_bookmark, update_bookmark, UpdatableBookmark, USER_CONTENT_ROOTS},
        history::frecency_stale_at,
        tags,
    };
    use crate::tests::{
        assert_json_tree as assert_local_json_tree, insert_json_tree as insert_local_json_tree,
    };
    use dogear::{Store as DogearStore, Validity};
    use pretty_assertions::assert_eq;
    use serde_json::{json, Value};
    use url::Url;

    use sync15::Payload;

    fn apply_incoming(conn: &PlacesDb, records_json: Value) {
        // suck records into the store.
        let interrupt_scope = conn.begin_interrupt_scope();
        let store = BookmarksStore::new(&conn, &interrupt_scope);

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
    }

    fn assert_incoming_creates_local_tree(
        api: &PlacesApi,
        records_json: Value,
        local_folder: &SyncGuid,
        local_tree: Value,
    ) {
        let conn = api
            .open_sync_connection()
            .expect("should get a sync connection");
        apply_incoming(&conn, records_json);
        assert_local_json_tree(&conn, local_folder, local_tree);
    }

    #[test]
    fn test_fetch_remote_tree() -> Result<()> {
        let _ = env_logger::try_init();
        let records = vec![
            json!({
                "id": "qqVTRWhLBOu3",
                "type": "bookmark",
                "parentid": "unfiled",
                "parentName": "Unfiled Bookmarks",
                "dateAdded": 1_381_542_355_843u64,
                "title": "The title",
                "bmkUri": "https://example.com",
                "tags": [],
            }),
            json!({
                "id": "unfiled",
                "type": "folder",
                "parentid": "places",
                "parentName": "",
                "dateAdded": 0,
                "title": "Unfiled Bookmarks",
                "children": ["qqVTRWhLBOu3"],
                "tags": [],
            }),
        ];

        let api = new_mem_api();
        let conn = api.open_sync_connection()?;

        // suck records into the store.
        let interrupt_scope = conn.begin_interrupt_scope();
        let store = BookmarksStore::new(&conn, &interrupt_scope);

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
        assert_eq!(tree.guids().count(), USER_CONTENT_ROOTS.len() + 2);

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
        let writer = api.open_connection(ConnectionType::ReadWrite)?;
        let syncer = api.open_sync_connection()?;

        writer
            .execute("UPDATE moz_bookmarks SET syncChangeCounter = 0", NO_PARAMS)
            .expect("should work");

        insert_local_json_tree(
            &writer,
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

        let interrupt_scope = syncer.begin_interrupt_scope();
        let store = BookmarksStore::new(&syncer, &interrupt_scope);
        let merger = Merger::new(&store, ServerTimestamp(0.0));

        let tree = merger.fetch_local_tree()?;

        // should be each user root, plus the real root, plus the bookmark we added.
        assert_eq!(tree.guids().count(), USER_CONTENT_ROOTS.len() + 2);

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
        let api = new_mem_api();
        assert_incoming_creates_local_tree(
            &api,
            json!([{
                "id": "bookmark1___",
                "type": "bookmark",
                "parentid": "unfiled",
                "parentName": "Unfiled Bookmarks",
                "dateAdded": 1_381_542_355_843u64,
                "title": "Some bookmark",
                "bmkUri": "http://example.com",
            },
            {
                "id": "unfiled",
                "type": "folder",
                "parentid": "root",
                "dateAdded": 1_381_542_355_843u64,
                "title": "Unfiled",
                "children": ["bookmark1___"],
            }]),
            &BookmarkRootGuid::Unfiled.as_guid(),
            json!({"children" : [{"guid": "bookmark1___", "url": "http://example.com"}]}),
        );
        let reader = api
            .open_connection(ConnectionType::ReadOnly)
            .expect("Should open read-only connection");
        assert!(
            frecency_stale_at(&reader, &Url::parse("http://example.com").unwrap())
                .expect("Should check stale frecency")
                .is_some(),
            "Should mark frecency for bookmark URL as stale"
        );

        let writer = api
            .open_connection(ConnectionType::ReadWrite)
            .expect("Should open read-write connection");
        insert_local_json_tree(
            &writer,
            json!({
                "guid": &BookmarkRootGuid::Menu.as_guid(),
                "children": [
                    {
                        "guid": "bookmark2___",
                        "title": "2",
                        "url": "http://example.com/2",
                    }
                ],
            }),
        );
        assert_incoming_creates_local_tree(
            &api,
            json!([{
                "id": "menu",
                "type": "folder",
                "parentid": "places",
                "parentName": "",
                "dateAdded": 0,
                "title": "menu",
                "children": ["bookmark2___"],
            }, {
                "id": "bookmark2___",
                "type": "bookmark",
                "parentid": "menu",
                "parentName": "menu",
                "dateAdded": 1_381_542_355_843u64,
                "title": "2",
                "bmkUri": "http://example.com/2-remote",
            }]),
            &BookmarkRootGuid::Menu.as_guid(),
            json!({"children" : [{"guid": "bookmark2___", "url": "http://example.com/2-remote"}]}),
        );
        assert!(
            frecency_stale_at(&reader, &Url::parse("http://example.com/2").unwrap())
                .expect("Should check stale frecency for old URL")
                .is_some(),
            "Should mark frecency for old URL as stale"
        );
        assert!(
            frecency_stale_at(&reader, &Url::parse("http://example.com/2-remote").unwrap())
                .expect("Should check stale frecency for new URL")
                .is_some(),
            "Should mark frecency for new URL as stale"
        );
    }

    #[test]
    fn test_apply_query() {
        // should we add some more query variations here?
        let api = new_mem_api();
        assert_incoming_creates_local_tree(
            &api,
            json!([{
                "id": "query1______",
                "type": "query",
                "parentid": "unfiled",
                "parentName": "Unfiled Bookmarks",
                "dateAdded": 1_381_542_355_843u64,
                "title": "Some query",
                "bmkUri": "place:tag=foo",
            },
            {
                "id": "unfiled",
                "type": "folder",
                "parentid": "root",
                "dateAdded": 1_381_542_355_843u64,
                "title": "Unfiled",
                "children": ["query1______"],
            }]),
            &BookmarkRootGuid::Unfiled.as_guid(),
            json!({"children" : [{"guid": "query1______", "url": "place:tag=foo"}]}),
        );
        let reader = api
            .open_connection(ConnectionType::ReadOnly)
            .expect("Should open read-only connection");
        assert!(
            frecency_stale_at(&reader, &Url::parse("place:tag=foo").unwrap())
                .expect("Should check stale frecency")
                .is_none(),
            "Should not mark frecency for queries as stale"
        );
    }

    #[test]
    fn test_apply() -> Result<()> {
        let api = new_mem_api();
        let writer = api.open_connection(ConnectionType::ReadWrite)?;
        let syncer = api.open_sync_connection()?;

        syncer
            .execute("UPDATE moz_bookmarks SET syncChangeCounter = 0", NO_PARAMS)
            .expect("should work");

        insert_local_json_tree(
            &writer,
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
            &writer,
            &Url::parse("http://example.com/a").expect("Should parse URL for A"),
            "baz",
        )
        .expect("Should tag A");

        let records = vec![
            json!({
                "id": "bookmarkCCCC",
                "type": "bookmark",
                "parentid": "menu",
                "parentName": "menu",
                "dateAdded": 1_552_183_116_885u64,
                "title": "C",
                "bmkUri": "http://example.com/c",
                "tags": ["foo", "bar"],
            }),
            json!({
                "id": "menu",
                "type": "folder",
                "parentid": "places",
                "parentName": "",
                "dateAdded": 0,
                "title": "menu",
                "children": ["bookmarkCCCC"],
            }),
        ];

        let interrupt_scope = syncer.begin_interrupt_scope();
        let store = BookmarksStore::new(&syncer, &interrupt_scope);

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
            &writer,
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
                                "date_added": Timestamp(1_552_183_116_885),
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
        let info_for_a = get_raw_bookmark(&writer, &guid_for_a)
            .expect("Should fetch info for A")
            .unwrap();
        assert_eq!(info_for_a.sync_change_counter, 1);
        let info_for_unfiled = get_raw_bookmark(&writer, &BookmarkRootGuid::Unfiled.as_guid())
            .expect("Should fetch info for unfiled")
            .unwrap();
        assert_eq!(info_for_unfiled.sync_change_counter, 1);

        store
            .sync_finished(
                ServerTimestamp(0.0),
                vec![
                    "bookmarkAAAA".into(),
                    "bookmarkBBBB".into(),
                    "unfiled".into(),
                ],
            )
            .expect("Should push synced changes back to the store");

        let info_for_a = get_raw_bookmark(&writer, &guid_for_a)
            .expect("Should fetch info for A")
            .unwrap();
        assert_eq!(info_for_a.sync_change_counter, 0);
        let info_for_unfiled = get_raw_bookmark(&writer, &BookmarkRootGuid::Unfiled.as_guid())
            .expect("Should fetch info for unfiled")
            .unwrap();
        assert_eq!(info_for_unfiled.sync_change_counter, 0);

        let mut tags_for_c = tags::get_tags_for_url(
            &writer,
            &Url::parse("http://example.com/c").expect("Should parse URL for C"),
        )
        .expect("Should return tags for C");
        tags_for_c.sort();
        assert_eq!(tags_for_c, &["bar", "foo"]);

        Ok(())
    }

    #[test]
    fn test_keywords() -> Result<()> {
        let api = new_mem_api();
        let writer = api.open_connection(ConnectionType::ReadWrite)?;
        let syncer = api.open_sync_connection()?;

        let records = vec![
            json!({
                "id": "toolbar",
                "type": "folder",
                "parentid": "places",
                "parentName": "",
                "dateAdded": 0,
                "title": "toolbar",
                "children": ["bookmarkAAAA"],
            }),
            json!({
                "id": "bookmarkAAAA",
                "type": "bookmark",
                "parentid": "toolbar",
                "parentName": "toolbar",
                "dateAdded": 1_552_183_116_885u64,
                "title": "A",
                "bmkUri": "http://example.com/a",
                "keyword": "a",
            }),
        ];

        let interrupt_scope = syncer.begin_interrupt_scope();
        let store = BookmarksStore::new(&syncer, &interrupt_scope);

        let mut incoming =
            IncomingChangeset::new(store.collection_name().to_string(), ServerTimestamp(0.0));
        for record in records {
            let payload = Payload::from_json(record).unwrap();
            incoming.changes.push((payload, ServerTimestamp(0.0)));
        }

        let outgoing = store
            .apply_incoming(incoming, &mut telemetry::EngineIncoming::new())
            .expect("Should apply incoming records");
        let mut outgoing_ids = outgoing
            .changes
            .iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>();
        outgoing_ids.sort();
        assert_eq!(outgoing_ids, &["menu", "mobile", "toolbar", "unfiled"],);

        store
            .sync_finished(ServerTimestamp(0.0), outgoing_ids)
            .expect("Should push synced changes back to the store");

        update_bookmark(
            &writer,
            &"bookmarkAAAA".into(),
            &UpdatableBookmark {
                title: Some("A (local)".into()),
                ..UpdatableBookmark::default()
            }
            .into(),
        )?;

        let outgoing = store
            .apply_incoming(
                IncomingChangeset::new(store.collection_name().to_string(), ServerTimestamp(1.0)),
                &mut telemetry::EngineIncoming::new(),
            )
            .expect("Should fetch outgoing records after making local changes");
        assert_eq!(outgoing.changes.len(), 1);
        assert_eq!(outgoing.changes[0].id, "bookmarkAAAA");
        assert_eq!(outgoing.changes[0].data["keyword"], "a");

        Ok(())
    }

    #[test]
    fn test_wipe() -> Result<()> {
        let api = new_mem_api();
        let writer = api.open_connection(ConnectionType::ReadWrite)?;
        let syncer = api.open_sync_connection()?;

        let records = vec![
            json!({
                "id": "toolbar",
                "type": "folder",
                "parentid": "places",
                "parentName": "",
                "dateAdded": 0,
                "title": "toolbar",
                "children": ["folderAAAAAA"],
            }),
            json!({
                "id": "folderAAAAAA",
                "type": "folder",
                "parentid": "toolbar",
                "parentName": "toolbar",
                "dateAdded": 0,
                "title": "A",
                "children": ["bookmarkBBBB"],
            }),
            json!({
                "id": "bookmarkBBBB",
                "type": "bookmark",
                "parentid": "folderAAAAAA",
                "parentName": "A",
                "dateAdded": 0,
                "title": "A",
                "bmkUri": "http://example.com/a",
            }),
            json!({
                "id": "menu",
                "type": "folder",
                "parentid": "places",
                "parentName": "",
                "dateAdded": 0,
                "title": "menu",
                "children": ["folderCCCCCC"],
            }),
            json!({
                "id": "folderCCCCCC",
                "type": "folder",
                "parentid": "menu",
                "parentName": "menu",
                "dateAdded": 0,
                "title": "A",
                "children": ["bookmarkDDDD", "folderEEEEEE"],
            }),
            json!({
                "id": "bookmarkDDDD",
                "type": "bookmark",
                "parentid": "folderCCCCCC",
                "parentName": "C",
                "dateAdded": 0,
                "title": "D",
                "bmkUri": "http://example.com/d",
            }),
            json!({
                "id": "folderEEEEEE",
                "type": "folder",
                "parentid": "folderCCCCCC",
                "parentName": "C",
                "dateAdded": 0,
                "title": "E",
                "children": ["bookmarkFFFF"],
            }),
            json!({
                "id": "bookmarkFFFF",
                "type": "bookmark",
                "parentid": "folderEEEEEE",
                "parentName": "E",
                "dateAdded": 0,
                "title": "F",
                "bmkUri": "http://example.com/f",
            }),
        ];

        let interrupt_scope = syncer.begin_interrupt_scope();
        let store = BookmarksStore::new(&syncer, &interrupt_scope);

        let mut incoming =
            IncomingChangeset::new(store.collection_name().to_string(), ServerTimestamp(0.0));
        for record in records {
            let payload = Payload::from_json(record).unwrap();
            incoming.changes.push((payload, ServerTimestamp(0.0)));
        }

        let outgoing = store
            .apply_incoming(incoming, &mut telemetry::EngineIncoming::new())
            .expect("Should apply incoming records");
        let mut outgoing_ids = outgoing
            .changes
            .iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>();
        outgoing_ids.sort();
        assert_eq!(outgoing_ids, &["menu", "mobile", "toolbar", "unfiled"],);

        store
            .sync_finished(ServerTimestamp(0.0), outgoing_ids)
            .expect("Should push synced changes back to the store");

        store.wipe().expect("Should wipe the store");

        // Wiping the store should delete all items except for the roots.
        assert_local_json_tree(
            &writer,
            &BookmarkRootGuid::Root.as_guid(),
            json!({
                "guid": &BookmarkRootGuid::Root.as_guid(),
                "children": [
                    {
                        "guid": &BookmarkRootGuid::Menu.as_guid(),
                        "children": [],
                    },
                    {
                        "guid": &BookmarkRootGuid::Toolbar.as_guid(),
                        "children": [],
                    },
                    {
                        "guid": &BookmarkRootGuid::Unfiled.as_guid(),
                        "children": [],
                    },
                    {
                        "guid": &BookmarkRootGuid::Mobile.as_guid(),
                        "children": [],
                    },
                ],
            }),
        );

        // Now pretend that F changed remotely between the time we called `wipe`
        // and the next sync.
        let record_for_f = json!({
            "id": "bookmarkFFFF",
            "type": "bookmark",
            "parentid": "folderEEEEEE",
            "parentName": "E",
            "dateAdded": 0,
            "title": "F (remote)",
            "bmkUri": "http://example.com/f-remote",
        });

        let mut incoming =
            IncomingChangeset::new(store.collection_name().to_string(), ServerTimestamp(1.0));
        incoming.changes.push((
            Payload::from_json(record_for_f).unwrap(),
            ServerTimestamp(1.0),
        ));

        let outgoing = store
            .apply_incoming(incoming, &mut telemetry::EngineIncoming::new())
            .expect("Should apply F and stage tombstones for A-E");
        let (outgoing_tombstones, outgoing_records): (Vec<_>, Vec<_>) =
            outgoing.changes.iter().partition(|record| record.deleted);
        let mut outgoing_record_ids = outgoing_records
            .into_iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>();
        outgoing_record_ids.sort();
        assert_eq!(
            outgoing_record_ids,
            &["bookmarkFFFF", "menu", "mobile", "toolbar", "unfiled"],
        );
        let mut outgoing_tombstone_ids = outgoing_tombstones
            .into_iter()
            .map(|p| p.id.clone())
            .collect::<Vec<_>>();
        outgoing_tombstone_ids.sort();
        assert_eq!(
            outgoing_tombstone_ids,
            &[
                "bookmarkBBBB",
                "bookmarkDDDD",
                "folderAAAAAA",
                "folderCCCCCC",
                "folderEEEEEE"
            ]
        );

        // F should move to the closest surviving ancestor, which, in this case,
        // is the menu.
        assert_local_json_tree(
            &writer,
            &BookmarkRootGuid::Root.as_guid(),
            json!({
                "guid": &BookmarkRootGuid::Root.as_guid(),
                "children": [
                    {
                        "guid": &BookmarkRootGuid::Menu.as_guid(),
                        "children": [
                            {
                                "guid": "bookmarkFFFF",
                                "title": "F (remote)",
                                "url": "http://example.com/f-remote",
                            },
                        ],
                    },
                    {
                        "guid": &BookmarkRootGuid::Toolbar.as_guid(),
                        "children": [],
                    },
                    {
                        "guid": &BookmarkRootGuid::Unfiled.as_guid(),
                        "children": [],
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

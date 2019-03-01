-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

-- This file defines triggers for the Sync connection.

CREATE TEMP TRIGGER noteItemRemoved
AFTER INSERT ON itemsToRemove
BEGIN
  /* Note that we can't record item removed notifications in the
     "removeLocalItems" trigger, because SQLite can delete rows in any
     order, and might fire the trigger for a removed parent before its
     children. */
  INSERT INTO itemsRemoved(itemId, parentId, position, type, placeId,
                           guid, parentGuid, level)
  SELECT b.id, b.parent, b.position, b.type, b.fk, b.guid, p.guid,
         NEW.localLevel
  FROM moz_bookmarks b
  JOIN moz_bookmarks p ON p.id = b.parent
  WHERE b.guid = NEW.guid;
END;

-- Removes items that are deleted on one or both sides from local items,
-- and inserts new tombstones for non-syncable items to delete remotely.
CREATE TEMP TRIGGER removeLocalItems
AFTER DELETE ON itemsToRemove
BEGIN
  /* Flag URL frecency for recalculation. */
  UPDATE moz_places SET
    frecency = -frecency
  WHERE id = (SELECT fk FROM moz_bookmarks
              WHERE guid = OLD.guid) AND
        frecency > 0;

  /* Trigger frecency updates for all affected origins. */
  DELETE FROM moz_updateoriginsupdate_temp;

  /* Remove annos for the deleted items. This can be removed in bug
     1460577. */
  DELETE FROM moz_items_annos
  WHERE item_id = (SELECT id FROM moz_bookmarks
                   WHERE guid = OLD.guid);

  /* Don't reupload tombstones for items that are already deleted on the
     server. */
  DELETE FROM moz_bookmarks_deleted
  WHERE NOT OLD.shouldUploadTombstone AND
        guid = OLD.guid;

  /* Upload tombstones for non-syncable items. We can remove the
     "shouldUploadTombstone" check and persist tombstones unconditionally
     in bug 1343103. */
  INSERT OR IGNORE INTO moz_bookmarks_deleted(guid, dateRemoved)
  SELECT OLD.guid, strftime('%s', 'now', 'localtime', 'utc') * 1000000
  WHERE OLD.shouldUploadTombstone;

  /* Remove the item from Places. */
  DELETE FROM moz_bookmarks
  WHERE guid = OLD.guid;

  /* Flag applied deletions as merged. */
  UPDATE moz_bookmarks_synced SET
    needsMerge = 0
  WHERE needsMerge AND
        guid = OLD.guid AND
        /* Don't flag tombstones for items that don't exist in the local
           tree. This can be removed once we persist tombstones in bug
           1343103. */
        (NOT isDeleted OR OLD.localLevel > -1);
END;

-- The bulk of the logic to apply all remotely changed bookmark items is
-- defined in `INSTEAD OF DELETE` triggers on the `itemsToMerge` and
-- `structureToMerge` views. When we execute `DELETE FROM
-- newRemote{Items, Structure}`, SQLite fires the triggers for each row in the
-- view. This is equivalent to, but more efficient than, issuing
-- `SELECT * FROM newRemote{Items, Structure}`, followed by separate
-- `INSERT` and `UPDATE` statements.

-- Changes local GUIDs to remote GUIDs, drops local tombstones for revived
-- remote items, and flags remote items as merged. In the trigger body, `OLD`
-- refers to the row for the unmerged item in `itemsToMerge`.
CREATE TEMP TRIGGER updateGuidsAndSyncFlags
INSTEAD OF DELETE ON itemsToMerge
BEGIN
  UPDATE moz_bookmarks SET
    /* We update GUIDs here, instead of in the "updateExistingLocalItems"
       trigger, because deduped items where we're keeping the local value
       state won't have "useRemote" set. */
    guid = OLD.mergedGuid,
    syncStatus = CASE WHEN OLD.useRemote
                 THEN 2 -- SyncStatus::Normal
                 ELSE syncStatus
                 END,
    /* Flag updated local items and new structure for upload. */
    syncChangeCounter = OLD.shouldUpload,
    lastModified = strftime('%s', 'now', 'localtime', 'utc') * 1000000
  WHERE id = OLD.localId;

  /* Record item changed notifications for the updated GUIDs. */
  INSERT INTO guidsChanged(itemId, oldGuid, level)
  SELECT OLD.localId, OLD.localGuid, OLD.newLevel
  WHERE OLD.localGuid <> OLD.mergedGuid;

  /* Drop local tombstones for revived remote items. */
  DELETE FROM moz_bookmarks_deleted
  WHERE guid IN (OLD.localGuid, OLD.remoteGuid);

  /* Flag the remote item as merged. */
  UPDATE moz_bookmarks_synced SET
    needsMerge = 0
  WHERE needsMerge AND
        guid IN (OLD.remoteGuid, OLD.localGuid);
END;

CREATE TEMP TRIGGER updateLocalItems
INSTEAD OF DELETE ON itemsToMerge WHEN OLD.useRemote
BEGIN
  /* Record an item added notification for the new item. */
  INSERT INTO itemsAdded(guid, keywordChanged, level)
  SELECT OLD.mergedGuid, OLD.newKeyword NOT NULL OR
                         EXISTS(SELECT 1 FROM moz_keywords
                                WHERE place_id = OLD.newPlaceId OR
                                      keyword = OLD.newKeyword),
         OLD.newLevel
  WHERE OLD.localId IS NULL;

  /* Record an item changed notification for the existing item. */
  INSERT INTO itemsChanged(itemId, oldTitle, oldPlaceId, keywordChanged,
                           level)
  SELECT id, title, OLD.oldPlaceId, OLD.newKeyword NOT NULL OR
           EXISTS(SELECT 1 FROM moz_keywords
                  WHERE place_id IN (OLD.oldPlaceId, OLD.newPlaceId) OR
                        keyword = OLD.newKeyword),
         OLD.newLevel
  FROM moz_bookmarks
  WHERE OLD.localId NOT NULL AND
        id = OLD.localId;

  /* Sync associates keywords with bookmarks, and doesn't sync POST data;
     Places associates keywords with (URL, POST data) pairs, and multiple
     bookmarks may have the same URL. For consistency (bug 1328737), we
     reupload all items with the old URL, new URL, and new keyword. Note
     that we intentionally use "k.place_id IN (...)" instead of
     "b.fk = OLD.newPlaceId OR fk IN (...)" in the WHERE clause because we
     only want to reupload items with keywords. */
  INSERT OR IGNORE INTO relatedIdsToReupload(id)
  SELECT b.id FROM moz_bookmarks b
  JOIN moz_keywords k ON k.place_id = b.fk
  WHERE (b.id <> OLD.localId OR OLD.localId IS NULL) AND (
          k.place_id IN (OLD.oldPlaceId, OLD.newPlaceId) OR
          k.keyword = OLD.newKeyword
        );

  /* Remove all keywords from the old and new URLs, and remove the new
     keyword from all existing URLs. */
  DELETE FROM moz_keywords WHERE place_id IN (OLD.oldPlaceId,
                                              OLD.newPlaceId) OR
                                 keyword = OLD.newKeyword;

  /* Remove existing tags. */
  DELETE FROM localTags WHERE placeId IN (OLD.oldPlaceId, OLD.newPlaceId);

  /* Insert the new item, using "-1" as the placeholder parent and
     position. We'll update these later, in the "updateLocalStructure"
     trigger. */
  INSERT INTO moz_bookmarks(id, guid, parent, position, type, fk, title,
                            dateAdded, lastModified, syncStatus,
                            syncChangeCounter)
  VALUES(OLD.localId, OLD.mergedGuid, -1, -1, OLD.newType, OLD.newPlaceId,
         OLD.newTitle, OLD.newDateAddedMicroseconds,
         strftime('%s', 'now', 'localtime', 'utc') * 1000000,
         2, -- SyncStatus::Normal
         OLD.shouldUpload)
  ON CONFLICT(id) DO UPDATE SET
    title = excluded.title,
    dateAdded = excluded.dateAdded,
    lastModified = excluded.lastModified,
    /* It's important that we update the URL *after* removing old keywords
       and *before* inserting new ones, so that the above DELETEs select
       the correct affected items. */
    fk = excluded.fk;

  /* Recalculate frecency. */
  UPDATE moz_places SET
    frecency = -frecency
  WHERE OLD.oldPlaceId <> OLD.newPlaceId AND
        id IN (OLD.oldPlaceId, OLD.newPlaceId) AND
        frecency > 0;

  /* Trigger frecency updates for all affected origins. */
  DELETE FROM moz_updateoriginsupdate_temp;

  /* Insert a new keyword for the new URL, if one is set. */
  INSERT OR IGNORE INTO moz_keywords(keyword, place_id, post_data)
  SELECT OLD.newKeyword, OLD.newPlaceId, ''
  WHERE OLD.newKeyword NOT NULL;

  /* Insert new tags for the new URL. */
  INSERT INTO localTags(tag, placeId)
  SELECT t.tag, OLD.newPlaceId FROM tags t
  WHERE t.itemId = OLD.remoteId;
END;

-- Updates all parents and positions to reflect the merged tree.
CREATE TEMP TRIGGER updateLocalStructure
INSTEAD OF DELETE ON structureToMerge
BEGIN
  UPDATE moz_bookmarks SET
    parent = OLD.newParentId
  WHERE id = OLD.localId AND
        parent <> OLD.newParentId;

  UPDATE moz_bookmarks SET
    position = OLD.newPosition
  WHERE id = OLD.localId AND
        position <> OLD.newPosition;

  /* Record observer notifications for moved items. We ignore items that
     didn't move, and items with placeholder parents and positions of "-1",
     since they're new. */
  INSERT INTO itemsMoved(itemId, oldParentId, oldParentGuid, oldPosition,
                         level)
  SELECT OLD.localId, OLD.oldParentId, p.guid, OLD.oldPosition,
         OLD.newLevel
  FROM moz_bookmarks p
  WHERE p.id = OLD.oldParentId AND
        -1 NOT IN (OLD.oldParentId, OLD.oldPosition) AND
        (OLD.oldParentId <> OLD.newParentId OR
         OLD.oldPosition <> OLD.newPosition);
END;

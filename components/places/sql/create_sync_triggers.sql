-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

-- This file defines triggers for the Sync connection.

--- Pushes uploaded changes back to the local and remote trees. This is more
--- or less equivalent to Desktop's `PlacesSyncUtils.bookmarks.pushChanges`.
CREATE TEMP TRIGGER pushUploadedChanges
AFTER UPDATE OF uploadedAt ON itemsToUpload WHEN NEW.uploadedAt > -1
BEGIN
  -- Reduce the change counter and update the sync status for uploaded items.
  -- If the item was uploaded during the sync, its change counter will still
  -- be > 0 for the next sync.
  UPDATE moz_bookmarks SET
      syncChangeCounter = max(syncChangeCounter - NEW.syncChangeCounter, 0),
      syncStatus = 2 -- SyncStatus::Normal
  WHERE guid = NEW.guid;

  -- Remove uploaded tombstones.
  DELETE FROM moz_bookmarks_deleted
  WHERE guid = NEW.guid;

  -- Write the uploaded item back to the synced bookmarks table, to match
  -- what's on the server now.
  REPLACE INTO moz_bookmarks_synced(guid, parentGuid, serverModified, needsMerge,
                                    validity, isDeleted, kind, dateAdded, title,
                                    placeId, keyword)
  VALUES(NEW.guid, NEW.parentGuid, NEW.uploadedAt, 0,
         1, -- SyncedBookmarkValidity::Valid
         NEW.isDeleted, NEW.kind, NEW.dateAdded, NEW.title,
         NEW.placeId, NEW.keyword);

  REPLACE INTO moz_bookmarks_synced_structure(guid, parentGuid, position)
  SELECT guid, NEW.guid, position
  FROM structureToUpload
  WHERE parentId = NEW.id;
END;

-- Removes items that are deleted on one or both sides from local items,
-- and inserts new tombstones for non-syncable items to delete remotely.
CREATE TEMP TRIGGER removeLocalItems
AFTER DELETE ON itemsToRemove
BEGIN
  -- Flag URL frecency for recalculation.
  UPDATE moz_places SET
      frecency = -frecency
  WHERE id = (SELECT fk FROM moz_bookmarks
              WHERE guid = OLD.guid) AND
        frecency > 0;

  -- Trigger frecency updates for all affected origins.
  DELETE FROM moz_updateoriginsupdate_temp;

  -- Don't reupload tombstones for items that are already deleted on the server.
  DELETE FROM moz_bookmarks_deleted
  WHERE NOT OLD.shouldUploadTombstone AND
        guid = OLD.guid;

  -- Upload tombstones for non-syncable items. `shouldUploadTombstone` can be
  -- removed if we ever persist tombstones (bug 1343103).
  INSERT OR IGNORE INTO moz_bookmarks_deleted(guid, dateRemoved)
  SELECT OLD.guid, now()
  WHERE OLD.shouldUploadTombstone;

  -- Remove the item from Places.
  DELETE FROM moz_bookmarks
  WHERE guid = OLD.guid;

  -- Flag applied deletions as merged.
  UPDATE moz_bookmarks_synced SET
      needsMerge = 0
  WHERE needsMerge AND
        guid = OLD.guid AND
        -- Don't flag tombstones for items that don't exist in the local
        -- tree. This check can be removed if we ever persist tombstones
        -- (bug 1343103).
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
      -- We update GUIDs here, instead of in the `updateExistingLocalItems`
      -- trigger, because deduped items with a local merge state won't have
      -- `useRemote` set.
      guid = OLD.mergedGuid,
      syncStatus = CASE WHEN OLD.useRemote
                   THEN 2 -- SyncStatus::Normal
                   ELSE syncStatus
                   END,
      -- Flag items with local and new structure merge states for upload.
      syncChangeCounter = OLD.shouldUpload,
      lastModified = now()
  WHERE id = OLD.localId;

  -- Drop local tombstones for revived remote items.
  DELETE FROM moz_bookmarks_deleted
  WHERE guid IN (OLD.localGuid, OLD.remoteGuid);

  -- Flag the remote item as merged.
  UPDATE moz_bookmarks_synced SET
      needsMerge = 0
  WHERE needsMerge AND
        guid IN (OLD.remoteGuid, OLD.localGuid);
END;

CREATE TEMP TRIGGER updateLocalItems
INSTEAD OF DELETE ON itemsToMerge WHEN OLD.useRemote
BEGIN
  -- Remove all existing tags.
  DELETE FROM moz_tags_relation
  WHERE place_id IN (OLD.oldPlaceId, OLD.newPlaceId);

  -- Insert the new item, using the Places root as the placeholder parent, and
  -- -1 as the position. We'll update these later, when we fire the
  -- `updateLocalStructure` trigger.
  INSERT INTO moz_bookmarks(id, guid, parent, position, type, fk, title,
                            dateAdded, lastModified, syncStatus,
                            syncChangeCounter)
  VALUES(OLD.localId, OLD.mergedGuid,
         (SELECT id FROM moz_bookmarks WHERE guid = "root________"), -1,
         OLD.newType, OLD.newPlaceId,
         OLD.newTitle, OLD.newDateAdded,
         now(),
         2, -- SyncStatus::Normal
         OLD.shouldUpload)
  ON CONFLICT(guid) DO UPDATE SET
      title = excluded.title,
      dateAdded = excluded.dateAdded,
      lastModified = excluded.lastModified,
      fk = excluded.fk;

  -- Flag frecency for recalculation.
  UPDATE moz_places SET
      frecency = -frecency
  WHERE OLD.oldPlaceId <> OLD.newPlaceId AND
        id IN (OLD.oldPlaceId, OLD.newPlaceId) AND
        frecency > 0;

  -- Trigger frecency updates for all affected origins.
  DELETE FROM moz_updateoriginsupdate_temp;

  -- Insert new tags for the new URL.
  INSERT INTO moz_tags_relation(tag_id, place_id)
  SELECT tagId, OLD.newPlaceId
  FROM moz_bookmarks_synced_tag_relation
  WHERE itemId = OLD.remoteId;
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
END;

-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

-- This file defines temp tables and views for the Sync connection.

-- Stores the merged tree structure, used to apply a merged bookmark
-- tree to the local store.
CREATE TEMP TABLE mergedTree(
    mergedGuid TEXT PRIMARY KEY,
    localGuid TEXT,
    remoteGuid TEXT,
    mergedParentGuid TEXT NOT NULL,
    level INTEGER NOT NULL,
    position INTEGER NOT NULL,
    useRemote BOOLEAN NOT NULL, -- Take the remote state?
    shouldUpload BOOLEAN NOT NULL, -- Flag the item for upload?
    mergedAt INTEGER NOT NULL, -- In milliseconds.
    -- The node should exist on at least one side.
    CHECK(localGuid NOT NULL OR remoteGuid NOT NULL)
) WITHOUT ROWID;

-- Stages all items to delete locally and remotely. Items to delete locally
-- don't need tombstones: since we took the remote deletion, the tombstone
-- already exists on the server. Items to delete remotely, or non-syncable
-- items to delete on both sides, need tombstones.
CREATE TEMP TABLE itemsToRemove(
    guid TEXT PRIMARY KEY,
    localLevel INTEGER NOT NULL,
    shouldUploadTombstone BOOLEAN NOT NULL,
    removedAt INTEGER NOT NULL -- In milliseconds.
) WITHOUT ROWID;

-- A view of all synced items. We use triggers on this view to update local
-- items. Note that we can't just `REPLACE INTO moz_bookmarks`, because
-- `REPLACE` doesn't fire the shared `AFTER DELETE` triggers that we use to
-- maintain schema coherency.
CREATE TEMP VIEW itemsToMerge(localId, localGuid, remoteId, remoteGuid,
                              mergedGuid, useRemote, shouldUpload, newLevel,
                              newType,
                              newDateAdded,
                              newTitle, oldPlaceId, newPlaceId,
                              newKeyword, mergedAt) AS
SELECT b.id, b.guid, v.id, v.guid,
       r.mergedGuid, r.useRemote, r.shouldUpload, r.level,
       (CASE WHEN v.kind IN (
                    1, -- SyncedBookmarkKind::Bookmark
                    2 -- SyncedBookmarkKind::Query
                  ) THEN 1 -- BookmarkType::Bookmark
             WHEN v.kind IN (
                    3, -- SyncedBookmarkKind::Folder
                    4 -- SyncedBookmarkKind::Livemark
                  ) THEN 2 -- BookmarkType::Folder
             ELSE 3 -- BookmarkType::Separator
             END),
       (CASE WHEN b.dateAdded < v.dateAdded THEN b.dateAdded
             ELSE v.dateAdded END),
       v.title, b.fk, v.placeId,
       v.keyword, r.mergedAt
FROM mergedTree r
LEFT JOIN moz_bookmarks_synced v ON v.guid = r.remoteGuid
LEFT JOIN moz_bookmarks b ON b.guid = r.localGuid
WHERE r.mergedGuid <> "root________";

-- A view of the structure states for all items in the merged tree.
CREATE TEMP VIEW structureToMerge(localId, oldParentId, newParentId,
                                  oldPosition, newPosition, newLevel) AS
SELECT b.id, b.parent, p.id, b.position, r.position, r.level
FROM moz_bookmarks b
JOIN mergedTree r ON r.mergedGuid = b.guid
JOIN moz_bookmarks p ON p.guid = r.mergedParentGuid
/* Don't reposition roots, since we never upload the Places root, and our
   merged tree doesn't have a tags root. */
WHERE "root________" NOT IN (r.mergedGuid, r.mergedParentGuid);

-- Stores local IDs for items to upload even if they're not flagged as changed
-- in Places. These are "weak" because we won't try to reupload the item on
-- the next sync if the upload is interrupted or fails.
CREATE TEMP TABLE idsToWeaklyUpload(
    id INTEGER PRIMARY KEY
);

-- Stores locally changed items staged for upload.
CREATE TEMP TABLE itemsToUpload(
    id INTEGER PRIMARY KEY,
    guid TEXT UNIQUE NOT NULL,
    syncChangeCounter INTEGER NOT NULL,
    -- The server modified time for the uploaded record. This is *not* a
    -- ServerTimestamp.
    uploadedAt INTEGER NOT NULL DEFAULT -1,
    isDeleted BOOLEAN NOT NULL DEFAULT 0,
    parentGuid TEXT,
    parentTitle TEXT,
    dateAdded INTEGER, -- In milliseconds.
    kind INTEGER,
    title TEXT,
    placeId INTEGER,
    url TEXT,
    keyword TEXT,
    position INTEGER
);

CREATE TEMP TABLE structureToUpload(
    guid TEXT PRIMARY KEY,
    parentId INTEGER NOT NULL REFERENCES itemsToUpload(id)
                              ON DELETE CASCADE,
    position INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TEMP TABLE tagsToUpload(
    id INTEGER REFERENCES itemsToUpload(id)
               ON DELETE CASCADE,
    tag TEXT,
    PRIMARY KEY(id, tag)
) WITHOUT ROWID;

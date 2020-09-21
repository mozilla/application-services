-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

-- This file defines triggers for the main read-write connection.

-- Unlike history, we manage bookmark tombstones via triggers. We do this
-- because we rely on foreign-keys to auto-remove children of a deleted folder.
CREATE TEMP TRIGGER moz_create_bookmarks_deleted_trigger
AFTER DELETE ON moz_bookmarks
FOR EACH ROW WHEN OLD.syncStatus = 2 -- SyncStatus::Normal
BEGIN
    INSERT into moz_bookmarks_deleted VALUES (OLD.guid, now());
END;

-- Updating the guid is only allowed by Sync, and it will use a connection
-- without some of these triggers - so for now we prevent changing the guid
-- of an existing item.
CREATE TEMP TRIGGER moz_remove_bookmarks_deleted_update_trigger
AFTER UPDATE ON moz_bookmarks
FOR EACH ROW WHEN OLD.guid != NEW.guid
BEGIN
    SELECT RAISE(FAIL, 'guids are immutable');
END;

-- These triggers bump the Sync change counter for all affected bookmarks when
-- a URL is tagged or untagged.
CREATE TEMP TRIGGER moz_tags_relations_afterinsert_sync_trigger
AFTER INSERT ON moz_tags_relation
BEGIN
    UPDATE moz_bookmarks SET
        syncChangeCounter = syncChangeCounter + 1
    WHERE fk = NEW.place_id;
END;

CREATE TEMP TRIGGER moz_tags_relations_afterupdate_sync_trigger
AFTER UPDATE ON moz_tags_relation
BEGIN
    UPDATE moz_bookmarks SET
        syncChangeCounter = syncChangeCounter + 1
    WHERE fk IN (OLD.place_id, NEW.place_id);
END;

CREATE TEMP TRIGGER moz_tags_relations_afterdelete_sync_trigger
AFTER DELETE ON moz_tags_relation
BEGIN
    UPDATE moz_bookmarks SET
        syncChangeCounter = syncChangeCounter + 1
    WHERE fk = OLD.place_id;
END;

-- Our "global" sync change counter.
-- It's "global" in the sense that it applies to all bookmarks across all
-- connections to the same DB.
CREATE TEMP TRIGGER moz_bookmarks_gscc_after_insert
AFTER INSERT ON moz_bookmarks FOR EACH ROW
BEGIN
    SELECT note_bookmarks_sync_change() WHERE NEW.syncChangeCounter > 0;
END;

CREATE TEMP TRIGGER moz_bookmarks_gscc_after_update
AFTER UPDATE OF fk, syncChangeCounter ON moz_bookmarks FOR EACH ROW
BEGIN
    SELECT note_bookmarks_sync_change()
    WHERE NEW.syncChangeCounter <> OLD.syncChangeCounter;
END;

-- Note that this will not capture deletions of bookmarks with a sync status of
-- "New" (because we don't tombstone them) whereas the other triggers above do,
-- but that's fine for this use-case, and we want the trigger on
-- moz_bookmarks_deleted to stay as close as possible to desktop.
CREATE TEMP TRIGGER moz_bookmarks_gscc_after_delete
AFTER INSERT ON moz_bookmarks_deleted FOR EACH ROW
BEGIN
    SELECT note_bookmarks_sync_change();
END;

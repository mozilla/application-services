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

-- Tags
CREATE TEMP TRIGGER moz_tags_relations_afterinsert_trigger
AFTER INSERT ON moz_tags_relation
BEGIN
    UPDATE moz_tags SET
        lastModified = now()
    WHERE id = NEW.tag_id;

    UPDATE moz_bookmarks SET
        syncChangeCounter = syncChangeCounter + 1
    WHERE fk = NEW.place_id;

    -- Tagging a URL increased the foreign count so that it will not be
    -- expired or otherwise automatically removed.
    UPDATE moz_places SET
        foreign_count = foreign_count + 1
    WHERE id = NEW.place_id;

END;


CREATE TEMP TRIGGER moz_tags_relations_afterupdate_trigger
AFTER UPDATE ON moz_tags_relation
BEGIN
    UPDATE moz_tags SET
        lastModified = now()
    WHERE id = NEW.tag_id;

    UPDATE moz_bookmarks SET
        syncChangeCounter = syncChangeCounter + 1
    WHERE fk IN (OLD.place_id, NEW.place_id);

    UPDATE moz_places SET
        foreign_count = foreign_count + 1
    WHERE id = NEW.place_id;

    UPDATE moz_places SET
        foreign_count = foreign_count - 1
    WHERE id = OLD.place_id;
END;

CREATE TEMP TRIGGER moz_tags_relations_afterdelete_trigger
AFTER DELETE ON moz_tags_relation
BEGIN
    UPDATE moz_tags SET
        lastModified = now()
    WHERE id = OLD.tag_id;

    UPDATE moz_bookmarks SET
        syncChangeCounter = syncChangeCounter + 1
    WHERE fk = OLD.place_id;

    UPDATE moz_places SET
        foreign_count = foreign_count - 1
    WHERE id = OLD.place_id;
END;

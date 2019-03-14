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

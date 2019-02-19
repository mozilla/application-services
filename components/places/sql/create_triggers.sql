-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

CREATE TEMP TRIGGER moz_places_afterinsert_trigger
AFTER INSERT ON moz_places FOR EACH ROW
BEGIN
    INSERT OR IGNORE INTO moz_origins(prefix, host, rev_host, frecency)
    VALUES(get_prefix(NEW.url), get_host_and_port(NEW.url), reverse_host(get_host_and_port(NEW.url)), NEW.frecency);

    -- This is temporary.
    UPDATE moz_places SET
      origin_id = (SELECT id FROM moz_origins
                   WHERE prefix = get_prefix(NEW.url) AND
                         host = get_host_and_port(NEW.url) AND
                         rev_host = reverse_host(get_host_and_port(NEW.url)))
    WHERE id = NEW.id;
END;

-- Note that while we create tombstones manually, we rely on this trigger to
-- delete any which might exist when a new record is written to moz_places.
CREATE TEMP TRIGGER moz_places_afterinsert_trigger_tombstone
AFTER INSERT ON moz_places
FOR EACH ROW
BEGIN
    DELETE FROM moz_places_tombstones WHERE guid = NEW.guid;
END;

-- Triggers which update visit_count and last_visit_date based on historyvisits
-- table changes.
-- NOTE: the values "0, 4, 7, 8, 9" below are EXCLUDED_VISIT_TYPES, stolen
-- from desktop.
CREATE TEMP TRIGGER moz_historyvisits_afterinsert_trigger
AFTER INSERT ON moz_historyvisits FOR EACH ROW
BEGIN
    UPDATE moz_places SET
        visit_count_remote = visit_count_remote + (NEW.visit_type NOT IN (0, 4, 7, 8, 9) AND NOT(NEW.is_local)),
        visit_count_local =  visit_count_local + (NEW.visit_type NOT IN (0, 4, 7, 8, 9) AND NEW.is_local),
        last_visit_date_local = MAX(last_visit_date_local,
                                    CASE WHEN NEW.is_local THEN NEW.visit_date ELSE 0 END),
        last_visit_date_remote = MAX(last_visit_date_remote,
                                     CASE WHEN NEW.is_local THEN 0 ELSE NEW.visit_date END)
    WHERE id = NEW.place_id;
END;

-- NOTE: the values "0, 4, 7, 8, 9" below are EXCLUDED_VISIT_TYPES, stolen
-- from desktop.
CREATE TEMP TRIGGER moz_historyvisits_afterdelete_trigger
AFTER DELETE ON moz_historyvisits FOR EACH ROW
BEGIN
    UPDATE moz_places SET
        visit_count_local = visit_count_local - (OLD.visit_type NOT IN (0, 4, 7, 8, 9) AND OLD.is_local),
        visit_count_remote = visit_count_remote - (OLD.visit_type NOT IN (0, 4, 7, 8, 9) AND NOT(OLD.is_local)),
        last_visit_date_local = IFNULL((SELECT visit_date FROM moz_historyvisits
                                        WHERE place_id = OLD.place_id AND is_local
                                        ORDER BY visit_date DESC LIMIT 1), 0),
        last_visit_date_remote = IFNULL((SELECT visit_date FROM moz_historyvisits
                                         WHERE place_id = OLD.place_id AND NOT(is_local)
                                         ORDER BY visit_date DESC LIMIT 1), 0)
    WHERE id = OLD.place_id;
END;

CREATE TEMP TRIGGER moz_bookmarks_foreign_count_afterdelete_trigger
AFTER DELETE ON moz_bookmarks FOR EACH ROW
BEGIN
    UPDATE moz_places
    SET foreign_count = foreign_count - 1
    WHERE id = OLD.fk;
END;

-- Note that the desktop versions of the triggers below call a note_sync_change()
-- function in some/all cases, which we will probably end up needing when we
-- come to sync.
CREATE TEMP TRIGGER moz_bookmarks_afterinsert_trigger
AFTER INSERT ON moz_bookmarks FOR EACH ROW
BEGIN
    UPDATE moz_places
        SET foreign_count = foreign_count + 1
        WHERE id = NEW.fk;
    DELETE from moz_bookmarks_deleted WHERE guid = NEW.guid;
END;

CREATE TEMP TRIGGER moz_bookmarks_foreign_count_afterupdate_trigger
AFTER UPDATE OF fk, syncChangeCounter ON moz_bookmarks FOR EACH ROW
BEGIN
    UPDATE moz_places
        SET foreign_count = foreign_count + 1
        WHERE OLD.fk <> NEW.fk AND id = NEW.fk;
    UPDATE moz_places
        SET foreign_count = foreign_count - 1
        WHERE OLD.fk <> NEW.fk AND id = OLD.fk;
END;

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

-- XXX - TODO - lots of desktop temp tables - but it's not clear they make sense here yet?
-- XXX - TODO - lots of favicon related tables - but it's not clear they make sense here yet?

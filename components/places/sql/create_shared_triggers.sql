-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

-- This file defines triggers shared between the main and Sync connections.

-- Note that while we create tombstones manually, we rely on this trigger to
-- delete any which might exist when a new record is written to moz_places.
CREATE TEMP TRIGGER moz_places_afterinsert_trigger_tombstone
AFTER INSERT ON moz_places
FOR EACH ROW
BEGIN
    DELETE FROM moz_places_tombstones WHERE guid = NEW.guid;
END;

-- Validate new rows. These were CHECK and FOREIGN KEY constraints
-- in schemas <= 17; a trigger lets us provide custom error messages.
CREATE TEMP TRIGGER moz_bookmarks_beforeinsert_trigger
BEFORE INSERT ON moz_bookmarks
BEGIN
    -- SQLite < 3.47.0 only supports string literals for error messages,
    -- so we use our own `throw(...)` function.

    SELECT throw(format('insert: len(guid)=%d', length(NEW.guid)))
    WHERE length(NEW.guid) <> 12;

    SELECT throw('insert: type=1; fk NULL')
    WHERE NEW.type = 1
      AND NEW.fk IS NULL;

    SELECT throw(format('insert: type=%d; fk NOT NULL', NEW.type))
    WHERE NEW.type <> 1
      AND NEW.fk IS NOT NULL;

    SELECT throw('insert: root with parent')
    WHERE NEW.guid = 'root________'
      AND NEW.parent IS NOT NULL;

    SELECT throw('insert: item without parent')
    WHERE NEW.guid <> 'root________'
      -- Equivalent to `FOREIGN KEY(parent) REFERENCES moz_bookmarks.parent`.
      AND NOT EXISTS(
          SELECT 1 FROM moz_bookmarks WHERE id = NEW.parent);
END;

CREATE TEMP TRIGGER moz_bookmarks_beforeupdate_trigger
BEFORE UPDATE ON moz_bookmarks
BEGIN
    SELECT throw(format('update: len(guid)=%d', length(NEW.guid)))
    WHERE length(NEW.guid) <> 12;

    SELECT throw('update: type=1; fk NULL')
    WHERE NEW.type = 1
      AND NEW.fk IS NULL;

    SELECT throw(format('update: type=%d; fk NOT NULL', NEW.type))
    WHERE NEW.type <> 1
      AND NEW.fk IS NOT NULL;

    SELECT throw('update: root with parent')
    WHERE NEW.guid = 'root________'
      AND NEW.parent IS NOT NULL;

    SELECT throw(format(
        'update: item without parent: operation=%q',
        CASE WHEN NEW.syncChangeCounter > 0 THEN 'sync' ELSE 'user' END
    ))
    WHERE NEW.guid <> 'root________'
      AND NEW.parent IS NULL;
-- bug 1941655, this seemingly more correct check causes obscure problems.
--      AND NOT EXISTS(
--          SELECT 1 FROM moz_bookmarks WHERE id = NEW.parent);

    SELECT throw(format('update: old type=%d; new=%d', OLD.type, NEW.type))
    WHERE OLD.type <> NEW.type;
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

-- The next several triggers are a workaround for the lack of FOR EACH STATEMENT
-- in Sqlite, (see bug 871908).
--
-- While doing inserts or deletes into moz_places, we accumulate the affected
-- origins into a temp table. Afterwards, we delete everything from the temp
-- table, causing the AFTER DELETE trigger to fire for it, which will then
-- update moz_origins and the origin frecency stats. As a consequence, we also
-- do this for updates to moz_places.frecency in order to make sure that changes
-- to origins are serialized.
--
-- Note this way we lose atomicity, crashing between the 2 queries may break the
-- tables' coherency. So it's better to run those DELETE queries in a single
-- transaction. Regardless, this is still better than hanging the browser for
-- several minutes on a fast machine.

-- Note: unlike the version of this trigger in desktop places, we don't bother with calling
-- store_last_inserted_id. Bug comments indicate that's only really needed because the hybrid
-- sync/async connection places prevents `last_insert_rowid` from working. This shouldn't be an
-- issue for us, and it's unclear how we'd implement `store_last_inserted_id` it while supporting
-- multiple connections to separate databases open simultaneously, which we'd like for testing
-- purposes. (To be clear, it's certainly possible to implement it if it turns out we need it, it
-- would just be very tricky).

CREATE TEMP TRIGGER moz_places_afterinsert_trigger_origins
AFTER INSERT ON moz_places FOR EACH ROW
BEGIN
    INSERT OR IGNORE INTO moz_updateoriginsinsert_temp (place_id, prefix, host, rev_host, frecency)
    VALUES (
        NEW.id,
        get_prefix(NEW.url),
        get_host_and_port(NEW.url),
        reverse_host(get_host_and_port(NEW.url)),
        NEW.frecency
    );
END;

-- This trigger corresponds to the previous trigger
-- (moz_places_afterinsert_trigger).  It runs on deletes on
-- moz_updateoriginsinsert_temp -- logically, after inserts on moz_places.
CREATE TEMP TRIGGER moz_updateoriginsinsert_afterdelete_trigger
AFTER DELETE ON moz_updateoriginsinsert_temp FOR EACH ROW
BEGIN
    -- Deduct the origin's current contribution to frecency stats
    {decrease_frecency_stats};

    INSERT INTO moz_origins (prefix, host, rev_host, frecency)
    VALUES (
        OLD.prefix,
        OLD.host,
        OLD.rev_host,
        MAX(OLD.frecency, 0)
    )
    ON CONFLICT(prefix, host) DO UPDATE
        SET frecency = frecency + OLD.frecency
        WHERE OLD.frecency > 0;

    -- Add the origin's new contribution to frecency stats
    {increase_frecency_stats};

    UPDATE moz_places SET origin_id = (
        SELECT id FROM moz_origins
        WHERE prefix = OLD.prefix
          AND host = OLD.host
    )
    WHERE id = OLD.place_id;
END;

-- When a row is deleted from places, we insert info about the frecency
-- delta into moz_updateoriginsdelete_tmp
CREATE TEMP TRIGGER moz_places_afterdelete_trigger_origins
AFTER DELETE ON moz_places
FOR EACH ROW
BEGIN
    INSERT INTO moz_updateoriginsdelete_temp (prefix, host, frecency_delta)
    VALUES (
        get_prefix(OLD.url),
        get_host_and_port(OLD.url),
        -MAX(OLD.frecency, 0)
    )
    ON CONFLICT(prefix, host) DO UPDATE
    SET frecency_delta = frecency_delta - OLD.frecency
    WHERE OLD.frecency > 0;
END;

-- This trigger corresponds to the previous trigger
-- (moz_places_afterdelete_trigger_origins).  It runs on deletes on
-- moz_updateoriginsdelete_temp -- logically, after deletes on moz_places.
CREATE TEMP TRIGGER moz_updateoriginsdelete_afterdelete_trigger
AFTER DELETE ON moz_updateoriginsdelete_temp FOR EACH ROW
BEGIN
    -- Deduct the origin's current contribution to frecency stats
    {decrease_frecency_stats};
    UPDATE moz_origins SET frecency = frecency + OLD.frecency_delta
    WHERE prefix = OLD.prefix AND host = OLD.host;

    DELETE FROM moz_origins
    WHERE prefix = OLD.prefix
        AND host = OLD.host
        AND NOT EXISTS (
            SELECT id FROM moz_places
            WHERE origin_id = moz_origins.id
            LIMIT 1
        );
    -- Add the origin's new contribution to frecency stats
    {increase_frecency_stats};
END;

-- Note: desktop places also has a notion of "frecency decay", and it only runs this
-- `WHEN NOT is_frecency_decaying()`.
CREATE TEMP TRIGGER moz_places_afterupdate_frecency_trigger
AFTER UPDATE OF frecency ON moz_places FOR EACH ROW
BEGIN
    INSERT INTO moz_updateoriginsupdate_temp (prefix, host, frecency_delta)
    VALUES (
        get_prefix(NEW.url),
        get_host_and_port(NEW.url),
        MAX(NEW.frecency, 0) - MAX(OLD.frecency, 0)
    )
    ON CONFLICT(prefix, host) DO UPDATE
    SET frecency_delta = frecency_delta + EXCLUDED.frecency_delta;
END;

-- This trigger corresponds to the previous trigger
-- (moz_places_afterupdate_frecency_trigger).  It runs on deletes on
-- moz_updateoriginsupdate_temp -- logically, after updates to places frecency.
CREATE TEMP TRIGGER moz_updateoriginsupdate_afterdelete_trigger
AFTER DELETE ON moz_updateoriginsupdate_temp FOR EACH ROW
BEGIN
    -- Deduct the origin's current contribution to frecency stats
    {decrease_frecency_stats};
    UPDATE moz_origins
    SET frecency = frecency + OLD.frecency_delta
    WHERE prefix = OLD.prefix
      AND host = OLD.host;
    -- Add the origin's new contribution to frecency stats
    {increase_frecency_stats};
END;

-- These triggers adjust the foreign count for synced bookmark URLs, so that
-- they won't be expired or automatically removed.
CREATE TEMP TRIGGER moz_bookmarks_synced_foreign_count_afterinsert_trigger
AFTER INSERT ON moz_bookmarks_synced
BEGIN
    UPDATE moz_places SET
        foreign_count = foreign_count + 1
    WHERE id = NEW.placeId;
END;

CREATE TEMP TRIGGER moz_bookmarks_synced_foreign_count_afterupdate_trigger
AFTER UPDATE OF placeId ON moz_bookmarks_synced
BEGIN
    UPDATE moz_places SET
        foreign_count = foreign_count + 1
    WHERE id = NEW.placeId;

    UPDATE moz_places SET
        foreign_count = foreign_count - 1
    WHERE id = OLD.placeId;
END;

CREATE TEMP TRIGGER moz_bookmarks_synced_foreign_count_afterdelete_trigger
AFTER DELETE ON moz_bookmarks_synced
BEGIN
    UPDATE moz_places SET
        foreign_count = foreign_count - 1
    WHERE id = OLD.placeId;
END;

-- Similar to cleanup_pages, if the origin/place remains with no foreign references
-- and no visits it should be deleted.
-- This approach may not be suitable for desktop but seems to be for us - see
-- https://bugzilla.mozilla.org/show_bug.cgi?id=1650511#c41 for more discussion.
CREATE TEMP TRIGGER moz_cleanup_origin_bookmark_deleted_trigger
AFTER DELETE ON moz_bookmarks
BEGIN
    DELETE FROM moz_places
        WHERE id = OLD.fk
        AND foreign_count = 0
        AND last_visit_date_local = 0
        AND last_visit_date_remote = 0;
END;

-- Equivalent to `FOREIGN KEY(parent) ... ON DELETE CASCADE`. Since the
-- BEFORE INSERT / UPDATE triggers already enforce the FOREIGN KEY
-- relationship, we use an AFTER DELETE trigger for the ON DELETE
-- action to avoid extra work.
CREATE TEMP TRIGGER moz_bookmarks_parent_afterdelete_trigger
AFTER DELETE ON moz_bookmarks
BEGIN
    DELETE FROM moz_bookmarks
    WHERE parent = OLD.id;
END;

-- These triggers adjust the foreign count for tagged URLs, and bump the
-- tag's last modified time when a URL is tagged or untagged. These are
-- split out from the main connection's tag triggers because we also want
-- this behavior when applying synced tags.
CREATE TEMP TRIGGER moz_tags_relations_afterinsert_trigger
AFTER INSERT ON moz_tags_relation
BEGIN
    UPDATE moz_tags SET
        lastModified = now()
    WHERE id = NEW.tag_id;

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
    WHERE id IN (OLD.tag_id, NEW.tag_id);

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

    UPDATE moz_places SET
        foreign_count = foreign_count - 1
    WHERE id = OLD.place_id;
END;

-- These triggers adjust the foreign count for URLs with keywords, so that
-- they won't be expired or automatically removed.

CREATE TEMP TRIGGER moz_keywords_afterinsert_trigger
AFTER INSERT ON moz_keywords
BEGIN
    UPDATE moz_places SET
        foreign_count = foreign_count + 1
    WHERE id = NEW.place_id;
END;

CREATE TEMP TRIGGER moz_keywords_afterupdate_trigger
AFTER UPDATE OF place_id ON moz_keywords
WHEN OLD.place_id <> NEW.place_id
BEGIN
    UPDATE moz_places SET
        foreign_count = foreign_count + 1
    WHERE id = NEW.place_id;

    UPDATE moz_places SET
        foreign_count = foreign_count - 1
    WHERE id = OLD.place_id;
END;

CREATE TEMP TRIGGER moz_keywords_afterdelete_trigger
AFTER DELETE ON moz_keywords
BEGIN
    UPDATE moz_places SET
        foreign_count = foreign_count - 1
    WHERE id = OLD.place_id;
END;

-- This trigger removes search query entries which no longer have any metadata records that point to them.
-- Due to SQLite's lack of 'FOR EACH STATEMENT' (only 'FOR EACH ROW' is supported), in case of bulk
-- deletes of metadata this will perform unnecessary SELECTs.
-- In other places, this is handled by "staging" temp tables.
CREATE TEMP TRIGGER moz_places_metadata_afterdelete_trigger_search_queries
AFTER DELETE ON moz_places_metadata
FOR EACH ROW
BEGIN
    DELETE FROM moz_places_metadata_search_queries WHERE id = OLD.search_query_id AND NOT EXISTS (
        SELECT id FROM moz_places_metadata pm WHERE pm.search_query_id = OLD.search_query_id
    );
END;

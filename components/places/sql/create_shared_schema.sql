-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

-- XXX - TODO - moz_annos
-- XXX - TODO - moz_anno_attributes
-- XXX - TODO - moz_items_annos

CREATE TABLE IF NOT EXISTS moz_places (
    id INTEGER PRIMARY KEY,
    url LONGVARCHAR NOT NULL,
    title LONGVARCHAR,
    -- note - desktop has rev_host here - that's now in moz_origin.
    visit_count_local INTEGER NOT NULL DEFAULT 0,
    visit_count_remote INTEGER NOT NULL DEFAULT 0,
    hidden INTEGER DEFAULT 0 NOT NULL,
    typed INTEGER DEFAULT 0 NOT NULL, -- XXX - is 'typed' ok? Note also we want this as a *count*, not a bool.
    frecency INTEGER DEFAULT -1 NOT NULL,
    -- XXX - splitting last visit into local and remote correct?
    last_visit_date_local INTEGER NOT NULL DEFAULT 0,
    last_visit_date_remote INTEGER NOT NULL DEFAULT 0,
    guid TEXT NOT NULL UNIQUE,
    foreign_count INTEGER DEFAULT 0 NOT NULL,
    url_hash INTEGER DEFAULT 0 NOT NULL,
    description TEXT, -- XXXX - title above?
    preview_image_url TEXT,
    -- origin_id would ideally be NOT NULL, but we use a trigger to keep
    -- it up to date, so do perform the initial insert with a null.
    origin_id INTEGER,
    -- a couple of sync-related fields.
    sync_status TINYINT NOT NULL DEFAULT 1, -- 1 is SyncStatus::New
    sync_change_counter INTEGER NOT NULL DEFAULT 0, -- adding visits will increment this

    FOREIGN KEY(origin_id) REFERENCES moz_origins(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS url_hashindex ON moz_places(url_hash);
CREATE INDEX IF NOT EXISTS visitcountlocal ON moz_places(visit_count_local);
CREATE INDEX IF NOT EXISTS visitcountremote ON moz_places(visit_count_remote);
CREATE INDEX IF NOT EXISTS frecencyindex ON moz_places(frecency);
CREATE INDEX IF NOT EXISTS lastvisitdatelocalindex ON moz_places(last_visit_date_local);
CREATE INDEX IF NOT EXISTS lastvisitdateremoteindex ON moz_places(last_visit_date_remote);
CREATE UNIQUE INDEX IF NOT EXISTS guid_uniqueindex ON moz_places(guid);
CREATE INDEX IF NOT EXISTS originidindex ON moz_places(origin_id);


CREATE TABLE IF NOT EXISTS moz_places_tombstones (
    guid TEXT PRIMARY KEY
) WITHOUT ROWID;


CREATE TABLE IF NOT EXISTS moz_historyvisits (
    id INTEGER PRIMARY KEY,
    is_local INTEGER NOT NULL, -- XXX - not in desktop - will always be true for visits added locally, always false visits added by sync.
    from_visit INTEGER, -- XXX - self-reference?
    place_id INTEGER NOT NULL,
    visit_date INTEGER NOT NULL,
    visit_type INTEGER NOT NULL,
    -- session INTEGER, -- XXX - what is 'session'? Appears unused.

    FOREIGN KEY(place_id) REFERENCES moz_places(id) ON DELETE CASCADE,
    FOREIGN KEY(from_visit) REFERENCES moz_historyvisits(id)
);

CREATE INDEX IF NOT EXISTS placedateindex ON moz_historyvisits(place_id, visit_date);
CREATE INDEX IF NOT EXISTS fromindex ON moz_historyvisits(from_visit);
CREATE INDEX IF NOT EXISTS dateindex ON moz_historyvisits(visit_date);
CREATE INDEX IF NOT EXISTS islocalindex ON moz_historyvisits(is_local);


CREATE TABLE IF NOT EXISTS moz_historyvisit_tombstones (
    place_id INTEGER NOT NULL,
    visit_date INTEGER NOT NULL,
    FOREIGN KEY(place_id) REFERENCES moz_places(id) ON DELETE CASCADE,
    PRIMARY KEY(place_id, visit_date)
);


CREATE TABLE IF NOT EXISTS moz_inputhistory (
    place_id INTEGER NOT NULL,
    input LONGVARCHAR NOT NULL,
    use_count INTEGER,

    PRIMARY KEY (place_id, input),
    FOREIGN KEY(place_id) REFERENCES moz_places(id) ON DELETE CASCADE
);


CREATE TABLE IF NOT EXISTS moz_bookmarks (
    id INTEGER PRIMARY KEY,
    fk INTEGER DEFAULT NULL, -- place_id
    type INTEGER NOT NULL,
    parent INTEGER,
    position INTEGER NOT NULL,
    title TEXT, -- a'la bug 1356159, NULL is special here - it means 'not edited'
    dateAdded INTEGER NOT NULL DEFAULT 0,
    lastModified INTEGER NOT NULL DEFAULT 0,
    guid TEXT NOT NULL UNIQUE CHECK(length(guid) == 12),

    syncStatus INTEGER NOT NULL DEFAULT 0,
    syncChangeCounter INTEGER NOT NULL DEFAULT 1,

    -- bookmarks must have a fk to a URL, other types must not.
    CHECK((type == 1 AND fk IS NOT NULL) OR (type > 1 AND fk IS NULL))
    -- only the root is allowed to have a non-null parent
    CHECK(guid == "root________" OR parent IS NOT NULL)

    FOREIGN KEY(fk) REFERENCES moz_places(id) ON DELETE RESTRICT
    FOREIGN KEY(parent) REFERENCES moz_bookmarks(id) ON DELETE CASCADE
);

-- CREATE INDEX IF NOT EXISTS itemindex ON moz_bookmarks(fk, type);
-- CREATE INDEX IF NOT EXISTS parentindex ON moz_bookmarks(parent, position);
CREATE INDEX IF NOT EXISTS itemlastmodifiedindex ON moz_bookmarks(fk, lastModified);
-- CREATE INDEX IF NOT EXISTS dateaddedindex ON moz_bookmarks(dateAdded);
CREATE UNIQUE INDEX IF NOT EXISTS guid_uniqueindex ON moz_bookmarks(guid);


CREATE TABLE IF NOT EXISTS moz_bookmarks_deleted (
    guid TEXT PRIMARY KEY,
    dateRemoved INTEGER NOT NULL
) WITHOUT ROWID;

-- Note: desktop has/had a 'keywords' table, but we intentionally do not.


CREATE TABLE IF NOT EXISTS moz_origins (
    id INTEGER PRIMARY KEY,
    prefix TEXT NOT NULL,
    host TEXT NOT NULL,
    rev_host TEXT NOT NULL,
    frecency INTEGER NOT NULL, -- XXX - why not default of -1 like in moz_places?
    UNIQUE (prefix, host)
);

CREATE INDEX IF NOT EXISTS hostindex ON moz_origins(rev_host);


-- This table holds key-value metadata for Places and its consumers. Sync stores
-- the sync IDs for the bookmarks and history collections in this table, and the
-- last sync time for history.
CREATE TABLE IF NOT EXISTS moz_meta (
    key TEXT PRIMARY KEY,
    value NOT NULL
) WITHOUT ROWID;

-- Support for tags.
CREATE TABLE moz_tags(
  id INTEGER PRIMARY KEY,
  tag TEXT UNIQUE NOT NULL,
  lastModified INTEGER NOT NULL
);

CREATE TABLE moz_tags_relation(
  tag_id INTEGER NOT NULL REFERENCES moz_tags(id) ON DELETE CASCADE,
  place_id INTEGER NOT NULL REFERENCES moz_places(id) ON DELETE CASCADE,
  PRIMARY KEY(tag_id, place_id)
) WITHOUT ROWID;

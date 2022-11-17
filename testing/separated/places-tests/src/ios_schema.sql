
-- Output of iOS `.schema` with unrelated entries filtered out.
CREATE TABLE IF NOT EXISTS "bookmarksBuffer" (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    guid TEXT NOT NULL UNIQUE,
    type TINYINT NOT NULL,
    server_modified INTEGER NOT NULL,
    is_deleted TINYINT NOT NULL DEFAULT 0,
    hasDupe TINYINT NOT NULL DEFAULT 0,
    parentid TEXT,
    parentName TEXT,
    feedUri TEXT,
    siteUri TEXT,
    pos INT,
    title TEXT,
    description TEXT,
    bmkUri TEXT,
    tags TEXT,
    keyword TEXT,
    folderName TEXT,
    queryId TEXT,
    date_added INTEGER,
    CONSTRAINT parentidOrDeleted CHECK (parentid IS NOT NULL OR is_deleted = 1),
    CONSTRAINT parentNameOrDeleted CHECK (parentName IS NOT NULL OR is_deleted = 1)
);

CREATE TABLE IF NOT EXISTS "bookmarksBufferStructure" (
    parent TEXT NOT NULL REFERENCES "bookmarksBuffer"(guid) ON DELETE CASCADE,
    child TEXT NOT NULL,
    idx INTEGER NOT NULL
);

CREATE TABLE bookmarksLocal (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    guid TEXT NOT NULL UNIQUE,
    type TINYINT NOT NULL,
    is_deleted TINYINT NOT NULL DEFAULT 0,
    parentid TEXT,
    parentName TEXT,
    feedUri TEXT,
    siteUri TEXT,
    pos INT,
    title TEXT,
    description TEXT,
    bmkUri TEXT,
    tags TEXT,
    keyword TEXT,
    folderName TEXT,
    queryId TEXT,
    local_modified INTEGER,
    sync_status TINYINT NOT NULL DEFAULT 0, -- NOTE(thom): I added default 0 here so we don't have to specify it.
    faviconID INTEGER, --REFERENCES favicons(id) ON DELETE SET NULL,
    date_added INTEGER,
    CONSTRAINT parentidOrDeleted CHECK (parentid IS NOT NULL OR is_deleted = 1),
    CONSTRAINT parentNameOrDeleted CHECK (parentName IS NOT NULL OR is_deleted = 1)
);

CREATE TABLE bookmarksMirror (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    guid TEXT NOT NULL UNIQUE,
    type TINYINT NOT NULL,
    is_deleted TINYINT NOT NULL DEFAULT 0,
    parentid TEXT,
    parentName TEXT,
    feedUri TEXT,
    siteUri TEXT,
    pos INT,
    title TEXT,
    description TEXT,
    bmkUri TEXT,
    tags TEXT,
    keyword TEXT,
    folderName TEXT,
    queryId TEXT,
    server_modified INTEGER NOT NULL,
    hasDupe TINYINT NOT NULL DEFAULT 0,
    is_overridden TINYINT NOT NULL DEFAULT 0,
    faviconID INTEGER, --REFERENCES favicons(id) ON DELETE SET NULL,
    date_added INTEGER,
    CONSTRAINT parentidOrDeleted CHECK (parentid IS NOT NULL OR is_deleted = 1),
    CONSTRAINT parentNameOrDeleted CHECK (parentName IS NOT NULL OR is_deleted = 1)
);

CREATE TABLE bookmarksLocalStructure (
    parent TEXT NOT NULL REFERENCES bookmarksLocal(guid) ON DELETE CASCADE,
    child TEXT NOT NULL,
    idx INTEGER NOT NULL
);

CREATE TABLE bookmarksMirrorStructure (
    parent TEXT NOT NULL REFERENCES bookmarksMirror(guid) ON DELETE CASCADE,
    child TEXT NOT NULL,
    idx INTEGER NOT NULL
);

CREATE INDEX idx_bookmarksBufferStructure_parent_idx ON bookmarksBufferStructure (parent, idx);
CREATE INDEX idx_bookmarksLocalStructure_parent_idx ON bookmarksLocalStructure (parent, idx);
CREATE INDEX idx_bookmarksMirrorStructure_parent_idx ON bookmarksMirrorStructure (parent, idx);

CREATE INDEX idx_bookmarksBuffer_keyword ON bookmarksBuffer (keyword);
CREATE INDEX idx_bookmarksLocal_keyword ON bookmarksLocal (keyword);
CREATE INDEX idx_bookmarksMirror_keyword ON bookmarksMirror (keyword);


-- History entries
CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    -- Not null, but the value might be replaced by the server's.
    guid TEXT NOT NULL UNIQUE,
    -- May only be null for deleted records.
    url TEXT UNIQUE,
    title TEXT NOT NULL,
    -- Can be null. Integer milliseconds.
    server_modified INTEGER,
    -- Can be null. Client clock. In extremis only.
    local_modified INTEGER,
    -- Boolean. Locally deleted.
    is_deleted TINYINT NOT NULL,
    -- Boolean. Set when changed or visits added.
    should_upload TINYINT NOT NULL,
    -- domain_id INTEGER REFERENCES domains(id) ON DELETE CASCADE,
    CONSTRAINT urlOrDeleted CHECK (url IS NOT NULL OR is_deleted = 1)
);

CREATE TABLE IF NOT EXISTS visits (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    siteID INTEGER NOT NULL REFERENCES history(id) ON DELETE CASCADE,
    -- Microseconds since epoch.
    date REAL NOT NULL,
    type INTEGER NOT NULL,
    -- Some visits are local. Some are remote ('mirrored'). This boolean flag is the split.
    is_local TINYINT NOT NULL,
    UNIQUE (siteID, date, type)
);

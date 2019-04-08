CREATE TABLE
IF NOT EXISTS push_record
(
    uaid               TEXT     NOT NULL,
    channel_id         TEXT     NOT NULL UNIQUE,
    endpoint           TEXT     NOT NULL UNIQUE,
    scope              TEXT     NOT NULL,
    key                TEXT     NOT NULL,
    ctime              INTEGER  NOT NULL,
    app_server_key     TEXT,
    native_id          TEXT,
    PRIMARY KEY
(uaid, channel_id)
);

DROP INDEX IF EXISTS channel_id_idx;
CREATE UNIQUE INDEX channel_id_idx ON push_record(channel_id);

CREATE TABLE
IF NOT EXISTS meta_data
(
    key                TEXT    NOT NULL UNIQUE PRIMARY KEY,
    value                      NOT NULL
) without ROWID;

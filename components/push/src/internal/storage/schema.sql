-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

CREATE TABLE
IF NOT EXISTS push_record
(
    channel_id         TEXT     NOT NULL PRIMARY KEY,
    -- `endpoint` must be unique; if 2 scopes ended up with the same endpoint, we'd possibly
    -- end up with a push message sent to the wrong observer.
    endpoint           TEXT     NOT NULL UNIQUE,
    scope              TEXT     NOT NULL UNIQUE,
    key                TEXT     NOT NULL,
    ctime              INTEGER  NOT NULL,
    app_server_key     TEXT,
    -- scope must have a value!
    CHECK(length(scope) > 0)
);

CREATE TABLE
IF NOT EXISTS meta_data
(
    key                TEXT    PRIMARY KEY,
    value                      NOT NULL
) without ROWID;

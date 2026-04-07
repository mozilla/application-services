-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

-- This table holds key-value metadata - primarily for sync.
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value NOT NULL
) WITHOUT ROWID;

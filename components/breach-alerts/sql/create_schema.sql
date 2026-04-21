-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

CREATE TABLE IF NOT EXISTS breach_alert_dismissals (
    -- The HIBP breach name. HIBP calls this a "name" rather than an ID, but it is unique
    -- and serves as the breach's stable identifier.
    breach_name TEXT NOT NULL PRIMARY KEY,
    -- Unix timestamp in milliseconds of when the breach alert was last dismissed.
    time_dismissed INTEGER NOT NULL
);

-- This table holds key-value metadata.
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value NOT NULL
) WITHOUT ROWID;

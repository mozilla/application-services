-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

CREATE TEMP TABLE addresses_sync_staging (
    guid                TEXT NOT NULL PRIMARY KEY CHECK(length(guid) != 0),
    payload             TEXT NOT NULL CHECK(length(payload) != 0)
);

CREATE TEMP TABLE credit_cards_sync_staging (
    guid                TEXT NOT NULL PRIMARY KEY CHECK(length(guid) != 0),
    payload             TEXT NOT NULL CHECK(length(payload) != 0)
);

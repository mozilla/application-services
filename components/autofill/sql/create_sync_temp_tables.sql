-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

CREATE TEMP TABLE addresses_staging (
    guid                TEXT NOT NULL PRIMARY KEY,
    given_name          TEXT NOT NULL,
    additional_name     TEXT NOT NULL,
    family_name         TEXT NOT NULL,
    organization        TEXT NOT NULL,
    street_address      TEXT NOT NULL,
    address_level3      TEXT NOT NULL,
    address_level2      TEXT NOT NULL,
    address_level1      TEXT NOT NULL,
    postal_code         TEXT NOT NULL,
    country             TEXT NOT NULL,
    tel                 TEXT NOT NULL,
    email               TEXT NOT NULL,

    -- time_created        INTEGER NOT NULL,
    -- time_last_used      INTEGER,
    -- time_last_modified  INTEGER NOT NULL,
    -- times_used          INTEGER NOT NULL DEFAULT 0
);

CREATE TEMP TABLE credit_cards_staging (
    guid                TEXT NOT NULL PRIMARY KEY,
    cc_name             TEXT NOT NULL,
    cc_number           TEXT NOT NULL,
    cc_exp_month        INTEGER,
    cc_exp_year         INTEGER,
    cc_type             TEXT NOT NULL,

    -- time_created        INTEGER NOT NULL,
    -- time_last_used      INTEGER,
    -- time_last_modified  INTEGER NOT NULL,
    -- times_used          INTEGER NOT NULL DEFAULT 0
);

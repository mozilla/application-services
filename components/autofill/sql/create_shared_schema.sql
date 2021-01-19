-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

CREATE TABLE IF NOT EXISTS addresses_data (
    guid                TEXT NOT NULL PRIMARY KEY,
    given_name          TEXT NOT NULL,
    additional_name     TEXT NOT NULL,
    family_name         TEXT NOT NULL,
    organization        TEXT NOT NULL,  -- Company
    street_address      TEXT NOT NULL,  -- (Multiline)
    address_level3      TEXT NOT NULL,  -- Suburb/Sublocality
    address_level2      TEXT NOT NULL,  -- City/Town
    address_level1      TEXT NOT NULL,  -- Province (Standardized code if possible)
    postal_code         TEXT NOT NULL,
    country             TEXT NOT NULL,  -- ISO 3166
    tel                 TEXT NOT NULL,  -- Stored in E.164 format
    email               TEXT NOT NULL,

    time_created        INTEGER NOT NULL,
    time_last_used      INTEGER,
    time_last_modified  INTEGER NOT NULL,
    times_used          INTEGER NOT NULL DEFAULT 0,

    sync_change_counter INTEGER NOT NULL DEFAULT 1
);

-- Note that we don't store tombstones in the mirror - maybe we should? That
-- would mean we need to change the schema here significantly - maybe we should
-- just store the JSON payload?
CREATE TABLE IF NOT EXISTS addresses_mirror (
   guid                 TEXT NOT NULL PRIMARY KEY,
    given_name          TEXT NOT NULL,
    additional_name     TEXT NOT NULL,
    family_name         TEXT NOT NULL,
    organization        TEXT NOT NULL,  -- Company
    street_address      TEXT NOT NULL,  -- (Multiline)
    address_level3      TEXT NOT NULL,  -- Suburb/Sublocality
    address_level2      TEXT NOT NULL,  -- City/Town
    address_level1      TEXT NOT NULL,  -- Province (Standardized code if possible)
    postal_code         TEXT NOT NULL,
    country             TEXT NOT NULL,  -- ISO 3166
    tel                 TEXT NOT NULL,  -- Stored in E.164 format
    email               TEXT NOT NULL,

    time_created        INTEGER NOT NULL,
    time_last_used      INTEGER,
    time_last_modified  INTEGER NOT NULL,
    times_used          INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS addresses_tombstones (
    guid            TEXT PRIMARY KEY,
    time_deleted    INTEGER NOT NULL
) WITHOUT ROWID;

-- XXX There are still questions around how we implement the necessary security model for credit cards, specifically
-- whether the `cc_number` and/or other details should be encrypted or stored as plain text. Currently, we are storing
-- them as plain text.
CREATE TABLE IF NOT EXISTS credit_cards_data (
    guid                TEXT NOT NULL PRIMARY KEY,
    cc_name             TEXT NOT NULL, -- full name
    cc_number           TEXT NOT NULL, -- TODO: consider storing this field as a hash
    cc_exp_month        INTEGER,
    cc_exp_year         INTEGER,
    cc_type             TEXT NOT NULL,

    time_created        INTEGER NOT NULL,
    time_last_used      INTEGER,
    time_last_modified  INTEGER NOT NULL,
    times_used          INTEGER NOT NULL DEFAULT 0,

    /* Same "sync change counter" strategy used by other components. */
    sync_change_counter INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS credit_cards_mirror (
    guid                TEXT NOT NULL PRIMARY KEY,
    cc_name             TEXT NOT NULL, -- full name
    cc_number           TEXT NOT NULL,
    cc_exp_month        INTEGER,
    cc_exp_year         INTEGER,
    cc_type             TEXT NOT NULL,

    time_created        INTEGER NOT NULL,
    time_last_used      INTEGER,
    time_last_modified  INTEGER NOT NULL,
    times_used          INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS credit_cards_tombstones (
    guid            TEXT PRIMARY KEY,
    time_deleted    INTEGER NOT NULL
) WITHOUT ROWID;

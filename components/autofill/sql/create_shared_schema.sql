-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

CREATE TABLE IF NOT EXISTS addresses_data (
    guid          TEXT NOT NULL PRIMARY KEY,
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

CREATE TABLE IF NOT EXISTS addresses_mirror (
   guid          TEXT NOT NULL PRIMARY KEY,
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

-- NOTE: The credit card tables below are not being implemented as there are still outstanding
-- questions around how we address security model.

-- CREATE TABLE IF NOT EXISTS credit_cards_data (
--     guid                TEXT NOT NULL PRIMARY KEY,
--     cc_name             TEXT NOT NULL, -- full name
--     cc_given_name       TEXT NOT NULL,
--     cc_additonal_name   TEXT NOT NULL,
--     cc_family_name      TEXT NOT NULL,
--     cc_number           TEXT NOT NULL,
--     cc_exp_month        INTEGER,
--     cc_exp_year         INTEGER,
--     cc_type             TEXT NOT NULL,
--     cc_exp              TEXT NOT NULL, -- text format of the expiration date e.g. "[cc_exp_year]-[cc_exp_month]"

--     time_created        INTEGER NOT NULL,
--     time_last_used      INTEGER,
--     time_last_modified  INTEGER NOT NULL,
--     times_used          INTEGER NOT NULL DEFAULT 0,

--     /* Same "sync change counter" strategy used by other components. */
--     sync_change_counter INTEGER NOT NULL DEFAULT 1
-- );

-- CREATE TABLE IF NOT EXISTS credit_cards_mirror (
--     guid                TEXT NOT NULL PRIMARY KEY,
--     cc_name             TEXT NOT NULL, -- full name
--     cc_given_name       TEXT NOT NULL,
--     cc_additonal_name   TEXT NOT NULL,
--     cc_family_name      TEXT NOT NULL,
--     cc_number           TEXT NOT NULL,
--     cc_exp_month        INTEGER,
--     cc_exp_year         INTEGER,
--     cc_type             TEXT NOT NULL,
--     cc_exp              TEXT NOT NULL, -- text format of the expiration date e.g. "[cc_exp_year]-[cc_exp_month]"

--     time_created        INTEGER NOT NULL,
--     time_last_used      INTEGER,
--     time_last_modified  INTEGER NOT NULL,
--     times_used          INTEGER NOT NULL DEFAULT 0
-- );

-- CREATE TABLE IF NOT EXISTS credit_cards_tombstones (
--     guid            TEXT PRIMARY KEY,
--     time_deleted    INTEGER NOT NULL
-- ) WITHOUT ROWID;

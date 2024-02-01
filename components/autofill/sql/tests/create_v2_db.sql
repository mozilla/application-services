-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

-- Initialize the v1 schema

CREATE TABLE IF NOT EXISTS addresses_data (
    guid                TEXT NOT NULL PRIMARY KEY CHECK(length(guid) != 0),
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
    time_last_used      INTEGER NOT NULL,
    time_last_modified  INTEGER NOT NULL,
    times_used          INTEGER NOT NULL,

    sync_change_counter INTEGER NOT NULL
);

-- What's on the server as the JSON payload.
CREATE TABLE IF NOT EXISTS addresses_mirror (
    guid                TEXT NOT NULL PRIMARY KEY CHECK(length(guid) != 0),
    payload             TEXT NOT NULL CHECK(length(payload) != 0)
    -- We could also have `modified`, which is in the server response and
    -- passed around in the sync code, but we don't have a use-case for using it.
);

-- Tombstones are items deleted locally but not deleted in the mirror (ie, ones
-- we are yet to upload)
CREATE TABLE IF NOT EXISTS addresses_tombstones (
    guid            TEXT PRIMARY KEY CHECK(length(guid) != 0),
    time_deleted    INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS credit_cards_data (
guid                TEXT NOT NULL PRIMARY KEY CHECK(length(guid) != 0),
cc_name             TEXT NOT NULL,
cc_number_enc       TEXT NOT NULL CHECK(length(cc_number_enc) > 20 OR cc_number_enc == ''),
cc_number_last_4    TEXT NOT NULL CHECK(length(cc_number_last_4) <= 4),
cc_exp_month        INTEGER,
cc_exp_year         INTEGER,
cc_type             TEXT NOT NULL,
time_created        INTEGER NOT NULL,
time_last_used      INTEGER,
time_last_modified  INTEGER NOT NULL,
times_used          INTEGER NOT NULL,
sync_change_counter INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS credit_cards_mirror (
    guid                TEXT NOT NULL PRIMARY KEY CHECK(length(guid) != 0),
    payload             TEXT NOT NULL CHECK(length(payload) != 0)
);

CREATE TABLE IF NOT EXISTS credit_cards_tombstones (
    guid            TEXT PRIMARY KEY CHECK(length(guid) != 0),
    time_deleted    INTEGER NOT NULL
) WITHOUT ROWID;

-- This table holds key-value metadata for the Autofill component and its consumers.
CREATE TABLE IF NOT EXISTS moz_meta (
    key TEXT PRIMARY KEY,
    value NOT NULL
) WITHOUT ROWID;

-- Populate it with some data, we test that this makes it through all the migrations.
INSERT INTO credit_cards_data (
    guid, cc_name, cc_number_enc, cc_number_last_4, cc_exp_month, cc_exp_year,
    cc_type, time_created, time_last_used, time_last_modified, times_used,
    sync_change_counter
) VALUES (
    "A", "Jane Doe", "012345678901234567890", "1234", 1, 2020, "visa", 0, 1, 2,
    3, 0
);

INSERT INTO addresses_data (
    guid, given_name, additional_name, family_name, organization,
    street_address, address_level3, address_level2, address_level1,
    postal_code, country, tel, email, time_created, time_last_used,
    time_last_modified, times_used, sync_change_counter
) VALUES (
    "A", "Jane", "John", "Doe", "Mozilla", "123 Maple lane", "Shelbyville",
    "Springfield", "MA", "12345", "US", "01-234-567-8000", "jane@hotmail.com", 0,
    1, 2, 3, 0
);

INSERT INTO addresses_data (
    guid, given_name, additional_name, family_name, organization,
    street_address, address_level3, address_level2, address_level1,
    postal_code, country, tel, email, time_created, time_last_used,
    time_last_modified, times_used, sync_change_counter
) VALUES (
    "B", "", "", "", "Mozilla", "123 Maple lane", "Shelbyville",
    "Springfield", "MA", "12345", "US", "01-234-567-8000", "jane@hotmail.com", 0,
    1, 2, 3, 0
);
PRAGMA user_version=2;

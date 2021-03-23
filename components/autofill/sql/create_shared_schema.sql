-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

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
    -- The plain-text sync15 payload.
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

-- XXX There are still questions around how we implement the necessary security model for credit cards, specifically
-- whether the `cc_number` and/or other details should be encrypted or stored as plain text. Currently, we are storing
-- them as plain text.
CREATE TABLE IF NOT EXISTS credit_cards_data (
    guid                TEXT NOT NULL PRIMARY KEY CHECK(length(guid) != 0),
    cc_name             TEXT NOT NULL, -- full name
    -- Encrypted card number, stored as a JWE. All valid unencrypted card
    -- numbers are 19 chars or less, and a base64 encoded JWE is always going to
    -- be longer than thus, so we add a CHECK designed to ensure we don't
    -- accidentally store unencrypted numbers here.
    cc_number_enc       TEXT NOT NULL CHECK(length(cc_number_enc) > 20),
    -- last 4 digits unencrypted. Check no larger than 4 to avoid the full number.
    cc_number_last_4    TEXT NOT NULL CHECK(length(cc_number_last_4) <= 4),
    cc_exp_month        INTEGER,
    cc_exp_year         INTEGER,
    cc_type             TEXT NOT NULL,

    time_created        INTEGER NOT NULL,
    time_last_used      INTEGER,
    time_last_modified  INTEGER NOT NULL,
    times_used          INTEGER NOT NULL,

    /* Same "sync change counter" strategy used by other components. */
    sync_change_counter INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS credit_cards_mirror (
    guid                TEXT NOT NULL PRIMARY KEY CHECK(length(guid) != 0),
    /*
    Note that:
    * The mirror needs to have a fully-formed Sync BSO payload, which
      includes the plaintext cc number.
    * But we don't want plaintext cc numbers on disk, so we have to encrypt the
      numbers in this payload - and the simplest way to do that is to just
      encrypt the entire payload.
    * The JWE encryption scheme is how we encrypt data for local storage, so
      that's what we use here (ie, we use the same encryption scheme that we
      use for `credit_cards_data.cc_number_enc`, and not the scheme Sync itself
      uses for encrypting payloads)
    */
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

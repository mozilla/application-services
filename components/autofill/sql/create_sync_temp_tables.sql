-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

CREATE TEMP TABLE addresses_sync_staging (
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
    email               TEXT NOT NULL
);

CREATE TEMP TABLE addresses_tombstone_sync_staging (
    guid                TEXT NOT NULL PRIMARY KEY
);

CREATE TEMP TABLE addresses_sync_applied (
    guid                    TEXT NOT NULL PRIMARY KEY,
    old_given_name          TEXT NULL,
    old_additional_name     TEXT NULL,
    old_family_name         TEXT NULL,
    old_organization        TEXT NULL,
    old_street_address      TEXT NULL,
    old_address_level3      TEXT NULL,
    old_address_level2      TEXT NULL,
    old_address_level1      TEXT NULL,
    old_postal_code         TEXT NULL,
    old_country             TEXT NULL,
    old_tel                 TEXT NULL,
    old_email               TEXT NULL,

    new_guid                TEXT NULL,
    new_given_name          TEXT NULL,
    new_additional_name     TEXT NULL,
    new_family_name         TEXT NULL,
    new_organization        TEXT NULL,
    new_street_address      TEXT NULL,
    new_address_level3      TEXT NULL,
    new_address_level2      TEXT NULL,
    new_address_level1      TEXT NULL,
    new_postal_code         TEXT NULL,
    new_country             TEXT NULL,
    new_tel                 TEXT NULL,
    new_email               TEXT NULL
);

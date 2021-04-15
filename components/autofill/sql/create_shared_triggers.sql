
-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

-- This file defines triggers shared between the main and Sync connections.

CREATE TEMP TRIGGER IF NOT EXISTS addresses_data_afterinsert_trigger
AFTER INSERT ON addresses_data
FOR EACH ROW WHEN NEW.guid IN (SELECT guid FROM addresses_tombstones)
BEGIN
    SELECT RAISE(FAIL, 'guid exists in `addresses_tombstones`');
END;

CREATE TEMP TRIGGER IF NOT EXISTS addresses_tombstones_afterinsert_trigger
AFTER INSERT ON addresses_tombstones
WHEN NEW.guid IN (SELECT guid FROM addresses_data)
BEGIN
    SELECT RAISE(FAIL, 'guid exists in `addresses_data`');
END;

CREATE TEMP TRIGGER IF NOT EXISTS addresses_tombstones_create_trigger
AFTER DELETE ON addresses_data
WHEN OLD.guid IN (SELECT guid FROM addresses_mirror)
BEGIN
    INSERT INTO addresses_tombstones(guid, time_deleted)
    VALUES (OLD.guid, now());
END;

CREATE TEMP TRIGGER IF NOT EXISTS credit_cards_data_afterinsert_trigger
AFTER INSERT ON credit_cards_data
FOR EACH ROW WHEN NEW.guid IN (SELECT guid FROM credit_cards_tombstones)
BEGIN
    SELECT RAISE(FAIL, 'guid exists in `credit_cards_tombstones`');
END;

CREATE TEMP TRIGGER IF NOT EXISTS credit_cards_tombstones_afterinsert_trigger
AFTER INSERT ON credit_cards_tombstones
WHEN NEW.guid IN (SELECT guid FROM credit_cards_data)
BEGIN
    SELECT RAISE(FAIL, 'guid exists in `credit_cards_data`');
END;

CREATE TEMP TRIGGER IF NOT EXISTS credit_cards_tombstones_create_trigger
AFTER DELETE ON credit_cards_data
WHEN OLD.guid IN (SELECT guid FROM credit_cards_mirror)
BEGIN
    INSERT INTO credit_cards_tombstones(guid, time_deleted)
    VALUES (OLD.guid, now());
END;

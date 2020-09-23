
-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at http://mozilla.org/MPL/2.0/.

-- This file defines triggers shared between the main and Sync connections.

CREATE TEMP TRIGGER addresses_data_afterinsert_trigger
AFTER INSERT ON addresses_data
FOR EACH ROW WHEN NEW.guid IN (SELECT guid FROM addresses_tombstones)
BEGIN
    SELECT RAISE(FAIL, 'guid exists in `addresses_tombstones`');
END;

CREATE TEMP TRIGGER addresses_tombstones_afterinsert_trigger
AFTER INSERT ON addresses_tombstones
WHEN NEW.guid IN (SELECT guid FROM addresses_data)
BEGIN
    SELECT RAISE(FAIL, 'guid exists in `addresses_data`');
END;

/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::{
    models::{
        address::{InternalAddress, UpdatableAddressFields},
        Metadata,
    },
    schema::{ADDRESS_COMMON_COLS, ADDRESS_COMMON_VALS},
};
use crate::error::*;

use rusqlite::{Connection, Transaction};
use sync_guid::Guid;
use types::Timestamp;

pub(crate) fn add_address(
    conn: &Connection,
    new: UpdatableAddressFields,
) -> Result<InternalAddress> {
    let tx = conn.unchecked_transaction()?;
    let now = Timestamp::now();

    // We return an InternalAddress, so set it up first, including the missing
    // fields, before we insert it.
    let address = InternalAddress {
        guid: Guid::random(),
        name: new.name,
        organization: new.organization,
        street_address: new.street_address,
        address_level3: new.address_level3,
        address_level2: new.address_level2,
        address_level1: new.address_level1,
        postal_code: new.postal_code,
        country: new.country,
        tel: new.tel,
        email: new.email,
        metadata: Metadata {
            time_created: now,
            time_last_modified: now,
            ..Default::default()
        },
    };
    add_internal_address(&tx, &address)?;
    tx.commit()?;
    Ok(address)
}

pub(crate) fn add_internal_address(tx: &Transaction<'_>, address: &InternalAddress) -> Result<()> {
    tx.execute(
        &format!(
            "INSERT INTO addresses_data (
                {common_cols},
                sync_change_counter
            ) VALUES (
                {common_vals},
                :sync_change_counter
            )",
            common_cols = ADDRESS_COMMON_COLS,
            common_vals = ADDRESS_COMMON_VALS,
        ),
        rusqlite::named_params! {
            ":guid": address.guid,
            ":name": address.name,
            ":organization": address.organization,
            ":street_address": address.street_address,
            ":address_level3": address.address_level3,
            ":address_level2": address.address_level2,
            ":address_level1": address.address_level1,
            ":postal_code": address.postal_code,
            ":country": address.country,
            ":tel": address.tel,
            ":email": address.email,
            ":time_created": address.metadata.time_created,
            ":time_last_used": address.metadata.time_last_used,
            ":time_last_modified": address.metadata.time_last_modified,
            ":times_used": address.metadata.times_used,
            ":sync_change_counter": address.metadata.sync_change_counter,
        },
    )?;
    Ok(())
}

pub(crate) fn get_address(conn: &Connection, guid: &Guid) -> Result<InternalAddress> {
    let sql = format!(
        "SELECT
            {common_cols},
            sync_change_counter
        FROM addresses_data
        WHERE guid = :guid",
        common_cols = ADDRESS_COMMON_COLS
    );
    conn.query_row(&sql, [guid], InternalAddress::from_row)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NoSuchRecord(guid.to_string()),
            e => e.into(),
        })
}

pub(crate) fn get_all_addresses(conn: &Connection) -> Result<Vec<InternalAddress>> {
    let sql = format!(
        "SELECT
            {common_cols},
            sync_change_counter
        FROM addresses_data",
        common_cols = ADDRESS_COMMON_COLS
    );

    let mut stmt = conn.prepare(&sql)?;
    let addresses = stmt
        .query_map([], InternalAddress::from_row)?
        .collect::<std::result::Result<Vec<InternalAddress>, _>>()?;
    Ok(addresses)
}

/// Updates just the "updatable" columns - suitable for exposure as a public
/// API.
pub(crate) fn update_address(
    conn: &Connection,
    guid: &Guid,
    address: &UpdatableAddressFields,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE addresses_data
        SET name                = :name,
            organization        = :organization,
            street_address      = :street_address,
            address_level3      = :address_level3,
            address_level2      = :address_level2,
            address_level1      = :address_level1,
            postal_code         = :postal_code,
            country             = :country,
            tel                 = :tel,
            email               = :email,
            sync_change_counter = sync_change_counter + 1
        WHERE guid              = :guid",
        rusqlite::named_params! {
            ":name": address.name,
            ":organization": address.organization,
            ":street_address": address.street_address,
            ":address_level3": address.address_level3,
            ":address_level2": address.address_level2,
            ":address_level1": address.address_level1,
            ":postal_code": address.postal_code,
            ":country": address.country,
            ":tel": address.tel,
            ":email": address.email,
            ":guid": guid,
        },
    )?;

    tx.commit()?;
    Ok(())
}

/// Updates all fields including metadata - although the change counter gets
/// slightly special treatment (eg, when called by Sync we don't want the
/// change counter incremented)
pub(crate) fn update_internal_address(
    tx: &Transaction<'_>,
    address: &InternalAddress,
    flag_as_changed: bool,
) -> Result<()> {
    let change_counter_increment = flag_as_changed as u32; // will be 1 or 0
    let rows_changed = tx.execute(
        "UPDATE addresses_data SET
            name                = :name,
            organization        = :organization,
            street_address      = :street_address,
            address_level3      = :address_level3,
            address_level2      = :address_level2,
            address_level1      = :address_level1,
            postal_code         = :postal_code,
            country             = :country,
            tel                 = :tel,
            email               = :email,
            time_created        = :time_created,
            time_last_used      = :time_last_used,
            time_last_modified  = :time_last_modified,
            times_used          = :times_used,
            sync_change_counter = sync_change_counter + :change_incr
        WHERE guid              = :guid",
        rusqlite::named_params! {
            ":name": address.name,
            ":organization": address.organization,
            ":street_address": address.street_address,
            ":address_level3": address.address_level3,
            ":address_level2": address.address_level2,
            ":address_level1": address.address_level1,
            ":postal_code": address.postal_code,
            ":country": address.country,
            ":tel": address.tel,
            ":email": address.email,
            ":time_created": address.metadata.time_created,
            ":time_last_used": address.metadata.time_last_used,
            ":time_last_modified": address.metadata.time_last_modified,
            ":times_used": address.metadata.times_used,
            ":change_incr": change_counter_increment,
            ":guid": address.guid,
        },
    )?;
    // Something went badly wrong if we are asking to update a row that doesn't
    // exist, or somehow we updated more than 1!
    assert_eq!(rows_changed, 1);
    Ok(())
}

pub(crate) fn delete_address(conn: &Connection, guid: &Guid) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;

    // execute returns how many rows were affected.
    let exists = tx.execute(
        "DELETE FROM addresses_data
            WHERE guid = :guid",
        rusqlite::named_params! {
            ":guid": guid,
        },
    )? != 0;
    tx.commit()?;
    Ok(exists)
}

pub fn touch(conn: &Connection, guid: &Guid) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let now_ms = Timestamp::now();

    tx.execute(
        "UPDATE addresses_data
        SET time_last_used              = :time_last_used,
            times_used                  = times_used + 1,
            sync_change_counter         = sync_change_counter + 1
        WHERE guid                      = :guid",
        rusqlite::named_params! {
            ":time_last_used": now_ms,
            ":guid": guid,
        },
    )?;

    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{schema::create_empty_sync_temp_tables, test::new_mem_db};
    use sync_guid::Guid;
    use types::Timestamp;

    #[allow(dead_code)]
    fn get_all(
        conn: &Connection,
        table_name: String,
    ) -> rusqlite::Result<Vec<String>, rusqlite::Error> {
        let mut stmt = conn.prepare(&format!(
            "SELECT guid FROM {table_name}",
            table_name = table_name
        ))?;
        let rows = stmt.query_map([], |row| row.get(0))?;

        let mut guids = Vec::new();
        for guid_result in rows {
            guids.push(guid_result?);
        }

        Ok(guids)
    }

    fn insert_tombstone_record(
        conn: &Connection,
        guid: String,
    ) -> rusqlite::Result<usize, rusqlite::Error> {
        conn.execute(
            "INSERT INTO addresses_tombstones (
                guid,
                time_deleted
            ) VALUES (
                :guid,
                :time_deleted
            )",
            rusqlite::named_params! {
                ":guid": guid,
                ":time_deleted": Timestamp::now(),
            },
        )
    }

    #[test]
    fn test_address_create_and_read() {
        let db = new_mem_db();

        let saved_address = add_address(
            &db,
            UpdatableAddressFields {
                name: "jane doe".to_string(),
                street_address: "123 Main Street".to_string(),
                address_level2: "Seattle, WA".to_string(),
                country: "United States".to_string(),

                ..UpdatableAddressFields::default()
            },
        )
        .expect("should contain saved address");

        // check that the add function populated the guid field
        assert_ne!(Guid::default(), saved_address.guid);

        // check that the time created and time last modified were set
        assert_ne!(0, saved_address.metadata.time_created.as_millis());
        assert_ne!(0, saved_address.metadata.time_last_modified.as_millis());

        assert_eq!(0, saved_address.metadata.sync_change_counter);

        // get created address
        let retrieved_address = get_address(&db, &saved_address.guid)
            .expect("should contain optional retrieved address");
        assert_eq!(saved_address.guid, retrieved_address.guid);
        assert_eq!(saved_address.name, retrieved_address.name);
        assert_eq!(
            saved_address.street_address,
            retrieved_address.street_address
        );
        assert_eq!(
            saved_address.address_level2,
            retrieved_address.address_level2
        );
        assert_eq!(saved_address.country, retrieved_address.country);

        // converting the created record into a tombstone to check that it's not returned on a second `get_address` call
        let delete_result = delete_address(&db, &saved_address.guid);
        assert!(delete_result.is_ok());
        assert!(delete_result.unwrap());

        assert!(get_address(&db, &saved_address.guid).is_err());
    }

    #[test]
    fn test_address_missing_guid() {
        let db = new_mem_db();
        let guid = Guid::random();
        let result = get_address(&db, &guid);

        assert_eq!(
            result.unwrap_err().to_string(),
            Error::NoSuchRecord(guid.to_string()).to_string()
        );
    }

    #[test]
    fn test_address_read_all() {
        let db = new_mem_db();

        let saved_address = add_address(
            &db,
            UpdatableAddressFields {
                name: "jane doe".to_string(),
                street_address: "123 Second Avenue".to_string(),
                address_level2: "Chicago, IL".to_string(),
                country: "United States".to_string(),

                ..UpdatableAddressFields::default()
            },
        )
        .expect("should contain saved address");

        let saved_address2 = add_address(
            &db,
            UpdatableAddressFields {
                name: "john deer".to_string(),
                street_address: "123 First Avenue".to_string(),
                address_level2: "Los Angeles, CA".to_string(),
                country: "United States".to_string(),

                ..UpdatableAddressFields::default()
            },
        )
        .expect("should contain saved address");

        // creating a third address with a tombstone to ensure it's not returned
        let saved_address3 = add_address(
            &db,
            UpdatableAddressFields {
                name: "abraham lincoln".to_string(),
                street_address: "1600 Pennsylvania Ave NW".to_string(),
                address_level2: "Washington, DC".to_string(),
                country: "United States".to_string(),

                ..UpdatableAddressFields::default()
            },
        )
        .expect("should contain saved address");

        let delete_result = delete_address(&db, &saved_address3.guid);
        assert!(delete_result.is_ok());
        assert!(delete_result.unwrap());

        let retrieved_addresses =
            get_all_addresses(&db).expect("Should contain all saved addresses");

        assert!(!retrieved_addresses.is_empty());
        let expected_number_of_addresses = 2;
        assert_eq!(expected_number_of_addresses, retrieved_addresses.len());

        let retrieved_address_guids = [
            retrieved_addresses[0].guid.as_str(),
            retrieved_addresses[1].guid.as_str(),
        ];
        assert!(retrieved_address_guids.contains(&saved_address.guid.as_str()));
        assert!(retrieved_address_guids.contains(&saved_address2.guid.as_str()));
    }

    #[test]
    fn test_address_update() {
        let db = new_mem_db();

        let saved_address = add_address(
            &db,
            UpdatableAddressFields {
                name: "john doe".to_string(),
                street_address: "1300 Broadway".to_string(),
                address_level2: "New York, NY".to_string(),
                country: "United States".to_string(),

                ..UpdatableAddressFields::default()
            },
        )
        .expect("should contain saved address");
        // change_counter starts at 0
        assert_eq!(0, saved_address.metadata.sync_change_counter);

        let expected_name = "john paul deer".to_string();
        let update_result = update_address(
            &db,
            &saved_address.guid,
            &UpdatableAddressFields {
                name: expected_name.clone(),
                organization: "".to_string(),
                street_address: "123 First Avenue".to_string(),
                address_level3: "".to_string(),
                address_level2: "Denver, CO".to_string(),
                address_level1: "".to_string(),
                postal_code: "".to_string(),
                country: "United States".to_string(),
                tel: "".to_string(),
                email: "".to_string(),
            },
        );
        assert!(update_result.is_ok());

        let updated_address =
            get_address(&db, &saved_address.guid).expect("should contain optional updated address");

        assert_eq!(saved_address.guid, updated_address.guid);
        assert_eq!(expected_name, updated_address.name);

        //check that the sync_change_counter was incremented
        assert_eq!(1, updated_address.metadata.sync_change_counter);
    }

    #[test]
    fn test_address_update_internal_address() -> Result<()> {
        let mut db = new_mem_db();
        let tx = db.transaction()?;

        let guid = Guid::random();
        add_internal_address(
            &tx,
            &InternalAddress {
                guid: guid.clone(),
                name: "john paul deer".to_string(),
                organization: "".to_string(),
                street_address: "123 First Avenue".to_string(),
                address_level3: "".to_string(),
                address_level2: "Denver, CO".to_string(),
                address_level1: "".to_string(),
                postal_code: "".to_string(),
                country: "United States".to_string(),
                tel: "".to_string(),
                email: "".to_string(),
                ..Default::default()
            },
        )?;

        let expected_name = "john paul dear";
        update_internal_address(
            &tx,
            &InternalAddress {
                guid: guid.clone(),
                name: expected_name.to_string(),
                organization: "".to_string(),
                street_address: "123 First Avenue".to_string(),
                address_level3: "".to_string(),
                address_level2: "Denver, CO".to_string(),
                address_level1: "".to_string(),
                postal_code: "".to_string(),
                country: "United States".to_string(),
                tel: "".to_string(),
                email: "".to_string(),
                ..Default::default()
            },
            false,
        )?;

        let record_exists: bool = tx.query_row(
            "SELECT EXISTS (
                SELECT 1
                FROM addresses_data
                WHERE guid = :guid
                AND name = :name
                AND sync_change_counter = 0
            )",
            [&guid.to_string(), &expected_name.to_string()],
            |row| row.get(0),
        )?;
        assert!(record_exists);

        Ok(())
    }

    #[test]
    fn test_address_delete() {
        fn num_tombstones(conn: &Connection) -> u32 {
            let stmt = "SELECT COUNT(*) from addresses_tombstones";
            conn.query_row(stmt, [], |row| Ok(row.get::<_, u32>(0).unwrap()))
                .unwrap()
        }

        let db = new_mem_db();
        create_empty_sync_temp_tables(&db).expect("should create temp tables");

        let saved_address = add_address(
            &db,
            UpdatableAddressFields {
                name: "jane doe".to_string(),
                street_address: "123 Second Avenue".to_string(),
                address_level2: "Chicago, IL".to_string(),
                country: "United States".to_string(),
                ..UpdatableAddressFields::default()
            },
        )
        .expect("first create should work");

        delete_address(&db, &saved_address.guid).expect("delete should work");
        // should be no tombstone as it wasn't in the mirror.
        assert_eq!(num_tombstones(&db), 0);

        // do it again, but with it in the mirror.
        let saved_address = add_address(
            &db,
            UpdatableAddressFields {
                name: "jane doe".to_string(),
                street_address: "123 Second Avenue".to_string(),
                address_level2: "Chicago, IL".to_string(),
                country: "United States".to_string(),
                ..UpdatableAddressFields::default()
            },
        )
        .expect("create 2nd address should work");
        db.execute(
            &format!(
                "INSERT INTO addresses_mirror (guid, payload) VALUES ('{}', 'whatever')",
                saved_address.guid,
            ),
            [],
        )
        .expect("manual insert into mirror");
        delete_address(&db, &saved_address.guid).expect("2nd delete");
        assert_eq!(num_tombstones(&db), 1);
    }

    #[test]
    fn test_address_trigger_on_create() {
        let db = new_mem_db();
        let tx = db.unchecked_transaction().expect("should get a tx");
        let guid = Guid::random();

        // create a tombstone record
        let tombstone_result = insert_tombstone_record(&db, guid.to_string());
        assert!(tombstone_result.is_ok());

        // create a new address with the tombstone's guid
        let address = InternalAddress {
            guid,
            name: "jane doe".to_string(),
            street_address: "123 Second Avenue".to_string(),
            address_level2: "Chicago, IL".to_string(),
            country: "United States".to_string(),
            ..Default::default()
        };

        let add_address_result = add_internal_address(&tx, &address);
        assert!(add_address_result.is_err());

        let expected_error_message = "guid exists in `addresses_tombstones`";
        assert!(add_address_result
            .unwrap_err()
            .to_string()
            .contains(expected_error_message))
    }

    #[test]
    fn test_address_trigger_on_delete() {
        let db = new_mem_db();
        let tx = db.unchecked_transaction().expect("should get a tx");
        let guid = Guid::random();

        // create an address
        let address = InternalAddress {
            guid,
            name: "jane doe".to_string(),
            street_address: "123 Second Avenue".to_string(),
            address_level2: "Chicago, IL".to_string(),
            country: "United States".to_string(),
            ..Default::default()
        };

        let add_address_result = add_internal_address(&tx, &address);
        assert!(add_address_result.is_ok());

        // create a tombstone record with the same guid
        let tombstone_result = insert_tombstone_record(&db, address.guid.to_string());
        assert!(tombstone_result.is_err());

        let expected_error_message = "guid exists in `addresses_data`";
        assert_eq!(
            expected_error_message,
            tombstone_result.unwrap_err().to_string()
        );
    }

    #[test]
    fn test_address_touch() -> Result<()> {
        let db = new_mem_db();
        let saved_address = add_address(
            &db,
            UpdatableAddressFields {
                name: "jane doe".to_string(),
                street_address: "123 Second Avenue".to_string(),
                address_level2: "Chicago, IL".to_string(),
                country: "United States".to_string(),

                ..UpdatableAddressFields::default()
            },
        )?;

        assert_eq!(saved_address.metadata.sync_change_counter, 0);
        assert_eq!(saved_address.metadata.times_used, 0);

        touch(&db, &saved_address.guid)?;

        let touched_address = get_address(&db, &saved_address.guid)?;

        assert_eq!(touched_address.metadata.sync_change_counter, 1);
        assert_eq!(touched_address.metadata.times_used, 1);

        Ok(())
    }
}

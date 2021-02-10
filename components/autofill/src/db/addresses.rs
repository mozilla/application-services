/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::{
    models::address::{InternalAddress, UpdatableAddressFields},
    schema::{ADDRESS_COMMON_COLS, ADDRESS_COMMON_VALS},
    store::{delete_meta, put_meta},
};
use crate::error::*;
use crate::sync::address::engine::{
    COLLECTION_SYNCID_META_KEY, GLOBAL_SYNCID_META_KEY, LAST_SYNC_META_KEY,
};

use rusqlite::{Connection, Transaction, NO_PARAMS};
use sync15::EngineSyncAssociation;
use sync_guid::Guid;
use types::Timestamp;

#[allow(dead_code)]
pub fn add_address(conn: &Connection, new: UpdatableAddressFields) -> Result<InternalAddress> {
    let tx = conn.unchecked_transaction()?;

    // We return an InternalAddress, so set it up first, including the missing
    // fields, before we insert it.
    let address = InternalAddress {
        guid: Guid::random(),
        given_name: new.given_name,
        additional_name: new.additional_name,
        family_name: new.family_name,
        organization: new.organization,
        street_address: new.street_address,
        address_level3: new.address_level3,
        address_level2: new.address_level2,
        address_level1: new.address_level1,
        postal_code: new.postal_code,
        country: new.country,
        tel: new.tel,
        email: new.email,
        time_created: Timestamp::now(),
        time_last_used: Some(Timestamp::now()),
        time_last_modified: Timestamp::now(),
        times_used: 0,
        sync_change_counter: 1,
    };

    tx.execute_named(
        &format!(
            "INSERT OR IGNORE INTO addresses_data (
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
            ":given_name": address.given_name,
            ":additional_name": address.additional_name,
            ":family_name": address.family_name,
            ":organization": address.organization,
            ":street_address": address.street_address,
            ":address_level3": address.address_level3,
            ":address_level2": address.address_level2,
            ":address_level1": address.address_level1,
            ":postal_code": address.postal_code,
            ":country": address.country,
            ":tel": address.tel,
            ":email": address.email,
            ":time_created": address.time_created,
            ":time_last_used": address.time_last_used,
            ":time_last_modified": address.time_last_modified,
            ":times_used": address.times_used,
            ":sync_change_counter": address.sync_change_counter,
        },
    )?;

    tx.commit()?;
    Ok(address)
}

#[allow(dead_code)]
pub fn get_address(conn: &Connection, guid: &Guid) -> Result<InternalAddress> {
    let tx = conn.unchecked_transaction()?;
    let sql = format!(
        "SELECT
            {common_cols},
            sync_change_counter
        FROM addresses_data
        WHERE guid = :guid",
        common_cols = ADDRESS_COMMON_COLS
    );

    let address = tx.query_row(&sql, &[guid], |row| Ok(InternalAddress::from_row(row)?))?;

    tx.commit()?;
    Ok(address)
}

#[allow(dead_code)]
pub fn get_all_addresses(conn: &Connection) -> Result<Vec<InternalAddress>> {
    let sql = format!(
        "SELECT
            {common_cols},
            sync_change_counter
        FROM addresses_data",
        common_cols = ADDRESS_COMMON_COLS
    );

    let mut stmt = conn.prepare(&sql)?;
    let addresses = stmt
        .query_map(NO_PARAMS, InternalAddress::from_row)?
        .collect::<std::result::Result<Vec<InternalAddress>, _>>()?;
    Ok(addresses)
}

#[allow(dead_code)]
pub fn update_address(
    conn: &Connection,
    guid: &Guid,
    address: &UpdatableAddressFields,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_named(
        "UPDATE addresses_data
        SET given_name         = :given_name,
            additional_name     = :additional_name,
            family_name         = :family_name,
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
            ":given_name": address.given_name,
            ":additional_name": address.additional_name,
            ":family_name": address.family_name,
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

pub fn delete_address(conn: &Connection, guid: &Guid) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;

    // execute_named returns how many rows were affected.
    let exists = tx.execute_named(
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

    tx.execute_named(
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

pub fn reset_in_tx(tx: &Transaction<'_>, assoc: &EngineSyncAssociation) -> Result<()> {
    // Remove all synced addresses and pending tombstones, and mark all
    // local addresses as new.
    tx.execute_batch(
        "DELETE FROM addresses_mirror;

        DELETE FROM addresses_tombstones;

        UPDATE addresses_data
        SET sync_change_counter = 1",
    )?;

    // Reset the last sync time, so that the next sync fetches fresh records
    // from the server.
    put_meta(tx, LAST_SYNC_META_KEY, &0)?;

    // Clear the sync ID if we're signing out, or set it to whatever the
    // server gave us if we're signing in.
    match assoc {
        EngineSyncAssociation::Disconnected => {
            delete_meta(tx, GLOBAL_SYNCID_META_KEY)?;
            delete_meta(tx, COLLECTION_SYNCID_META_KEY)?;
        }
        EngineSyncAssociation::Connected(ids) => {
            put_meta(tx, GLOBAL_SYNCID_META_KEY, &ids.global)?;
            put_meta(tx, COLLECTION_SYNCID_META_KEY, &ids.coll)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{schema::create_empty_sync_temp_tables, store::get_meta, test::new_mem_db};
    use sql_support::ConnExt;
    use sync15::CollSyncIds;
    use sync_guid::Guid;
    use types::Timestamp;

    fn get_all(
        conn: &Connection,
        table_name: String,
    ) -> rusqlite::Result<Vec<String>, rusqlite::Error> {
        let mut stmt = conn.prepare(&format!(
            "SELECT guid FROM {table_name}",
            table_name = table_name
        ))?;
        let rows = stmt.query_map(NO_PARAMS, |row| row.get(0))?;

        let mut guids = Vec::new();
        for guid_result in rows {
            guids.push(guid_result?);
        }

        Ok(guids)
    }

    fn clear_tables(conn: &Connection) -> rusqlite::Result<(), rusqlite::Error> {
        conn.execute_all(&[
            "DELETE FROM addresses_data;",
            "DELETE FROM addresses_mirror;",
            "DELETE FROM addresses_tombstones;",
            "DELETE FROM moz_meta;",
        ])
    }

    fn insert_tombstone_record(
        conn: &Connection,
        guid: String,
    ) -> rusqlite::Result<usize, rusqlite::Error> {
        conn.execute_named(
            "INSERT OR IGNORE INTO addresses_tombstones (
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

    fn insert_mirror_record(
        conn: &Connection,
        address: &InternalAddress,
    ) -> rusqlite::Result<usize, rusqlite::Error> {
        conn.execute_named(
            &format!(
                "INSERT OR IGNORE INTO addresses_mirror (
                {common_cols}
            ) VALUES (
                {common_vals}
            )",
                common_cols = ADDRESS_COMMON_COLS,
                common_vals = ADDRESS_COMMON_VALS
            ),
            rusqlite::named_params! {
                ":guid": address.guid,
                ":given_name": address.given_name,
                ":additional_name": address.additional_name,
                ":family_name": address.family_name,
                ":organization": address.organization,
                ":street_address": address.street_address,
                ":address_level3": address.address_level3,
                ":address_level2": address.address_level2,
                ":address_level1": address.address_level1,
                ":postal_code": address.postal_code,
                ":country": address.country,
                ":tel": address.tel,
                ":email": address.email,
                ":time_created": address.time_created,
                ":time_last_used": address.time_last_used,
                ":time_last_modified": address.time_last_modified,
                ":times_used": address.times_used,
            },
        )
    }

    fn insert_record(
        conn: &Connection,
        address: &InternalAddress,
    ) -> rusqlite::Result<usize, rusqlite::Error> {
        conn.execute_named(
            &format!(
                "INSERT OR IGNORE INTO addresses_data (
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
                ":given_name": address.given_name,
                ":additional_name": address.additional_name,
                ":family_name": address.family_name,
                ":organization": address.organization,
                ":street_address": address.street_address,
                ":address_level3": address.address_level3,
                ":address_level2": address.address_level2,
                ":address_level1": address.address_level1,
                ":postal_code": address.postal_code,
                ":country": address.country,
                ":tel": address.tel,
                ":email": address.email,
                ":time_created": address.time_created,
                ":time_last_used": address.time_last_used,
                ":time_last_modified": address.time_last_modified,
                ":times_used": address.times_used,
                ":sync_change_counter": address.sync_change_counter,
            },
        )
    }

    #[test]
    fn test_address_create_and_read() {
        let db = new_mem_db();

        let saved_address = add_address(
            &db,
            UpdatableAddressFields {
                given_name: "jane".to_string(),
                family_name: "doe".to_string(),
                street_address: "123 Main Street".to_string(),
                address_level2: "Seattle, WA".to_string(),
                country: "United States".to_string(),

                ..UpdatableAddressFields::default()
            },
        )
        .expect("should contain saved address");

        // check that the add function populated the guid field
        assert_ne!(Guid::default(), saved_address.guid);

        // check that sync_change_counter was set
        assert_eq!(1, saved_address.sync_change_counter);

        // get created address
        let retrieved_address = get_address(&db, &saved_address.guid)
            .expect("should contain optional retrieved address");
        assert_eq!(saved_address.guid, retrieved_address.guid);
        assert_eq!(saved_address.given_name, retrieved_address.given_name);
        assert_eq!(saved_address.family_name, retrieved_address.family_name);
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
    fn test_address_read_all() {
        let db = new_mem_db();

        let saved_address = add_address(
            &db,
            UpdatableAddressFields {
                given_name: "jane".to_string(),
                family_name: "doe".to_string(),
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
                given_name: "john".to_string(),
                family_name: "deer".to_string(),
                street_address: "123 First Avenue".to_string(),
                address_level2: "Los Angeles, CA".to_string(),
                country: "United States".to_string(),

                ..UpdatableAddressFields::default()
            },
        )
        .expect("should contain saved address");

        // creating a third address with a tombstone to ensure it's not retunred
        let saved_address3 = add_address(
            &db,
            UpdatableAddressFields {
                given_name: "abraham".to_string(),
                family_name: "lincoln".to_string(),
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

        let retrieved_address_guids = vec![
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
                given_name: "john".to_string(),
                family_name: "doe".to_string(),
                street_address: "1300 Broadway".to_string(),
                address_level2: "New York, NY".to_string(),
                country: "United States".to_string(),

                ..UpdatableAddressFields::default()
            },
        )
        .expect("should contain saved address");

        let expected_additional_name = "paul".to_string();
        let update_result = update_address(
            &db,
            &saved_address.guid,
            &UpdatableAddressFields {
                given_name: "john".to_string(),
                additional_name: expected_additional_name.clone(),
                family_name: "deer".to_string(),
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
        assert_eq!(expected_additional_name, updated_address.additional_name);

        //check that the sync_change_counter was incremented
        assert_eq!(2, updated_address.sync_change_counter);
    }

    #[test]
    fn test_address_delete() {
        fn num_tombstones(conn: &Connection) -> u32 {
            let stmt = "SELECT COUNT(*) from addresses_tombstones";
            conn.query_row(stmt, NO_PARAMS, |row| Ok(row.get::<_, u32>(0).unwrap()))
                .unwrap()
        }

        let db = new_mem_db();
        create_empty_sync_temp_tables(&db).expect("should create temp tables");

        let saved_address = add_address(
            &db,
            UpdatableAddressFields {
                given_name: "jane".to_string(),
                family_name: "doe".to_string(),
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
                given_name: "jane".to_string(),
                family_name: "doe".to_string(),
                street_address: "123 Second Avenue".to_string(),
                address_level2: "Chicago, IL".to_string(),
                country: "United States".to_string(),
                ..UpdatableAddressFields::default()
            },
        )
        .expect("create 2nd address should work");
        db.execute(
            &format!(
                "INSERT INTO addresses_mirror ({cols})
                 SELECT {cols} FROM addresses_data;",
                cols = ADDRESS_COMMON_COLS
            ),
            NO_PARAMS,
        )
        .expect("manual insert into mirror");
        delete_address(&db, &saved_address.guid).expect("2nd delete");
        assert_eq!(num_tombstones(&db), 1);
    }

    #[test]
    fn test_address_trigger_on_create() {
        let db = new_mem_db();
        let guid = Guid::random();

        // create a tombstone record
        let tombstone_result = insert_tombstone_record(&db, guid.to_string());
        assert!(tombstone_result.is_ok());

        // create a new address with the tombstone's guid
        let address = InternalAddress {
            guid,
            given_name: "jane".to_string(),
            family_name: "doe".to_string(),
            street_address: "123 Second Avenue".to_string(),
            address_level2: "Chicago, IL".to_string(),
            country: "United States".to_string(),
            ..Default::default()
        };

        let add_address_result = insert_record(&db, &address);
        assert!(add_address_result.is_err());

        let expected_error_message = "guid exists in `addresses_tombstones`";
        assert_eq!(
            expected_error_message,
            add_address_result.unwrap_err().to_string()
        );
    }

    #[test]
    fn test_address_trigger_on_delete() {
        let db = new_mem_db();
        let guid = Guid::random();

        // create an address
        let address = InternalAddress {
            guid,
            given_name: "jane".to_string(),
            family_name: "doe".to_string(),
            street_address: "123 Second Avenue".to_string(),
            address_level2: "Chicago, IL".to_string(),
            country: "United States".to_string(),
            ..Default::default()
        };

        let add_address_result = insert_record(&db, &address);
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
                given_name: "jane".to_string(),
                family_name: "doe".to_string(),
                street_address: "123 Second Avenue".to_string(),
                address_level2: "Chicago, IL".to_string(),
                country: "United States".to_string(),

                ..UpdatableAddressFields::default()
            },
        )?;

        assert_eq!(saved_address.sync_change_counter, 1);
        assert_eq!(saved_address.times_used, 0);

        touch(&db, &saved_address.guid)?;

        let touched_address = get_address(&db, &saved_address.guid)?;

        assert_eq!(touched_address.sync_change_counter, 2);
        assert_eq!(touched_address.times_used, 1);

        Ok(())
    }

    #[test]
    fn test_address_sync_reset() -> Result<()> {
        let mut db = new_mem_db();
        let tx = &db.transaction()?;

        // create a record
        let address = InternalAddress {
            guid: Guid::random(),
            sync_change_counter: 0,
            given_name: "jane".to_string(),
            family_name: "doe".to_string(),
            street_address: "123 Second Avenue".to_string(),
            address_level2: "Chicago, IL".to_string(),
            country: "United States".to_string(),

            ..InternalAddress::default()
        };
        insert_record(&tx, &address)?;

        // create a mirror record
        let mirror_record = InternalAddress {
            guid: Guid::random(),
            given_name: "jane".to_string(),
            family_name: "doe".to_string(),
            street_address: "123 Second Avenue".to_string(),
            address_level2: "Chicago, IL".to_string(),
            country: "United States".to_string(),

            ..InternalAddress::default()
        };
        insert_mirror_record(&tx, &mirror_record)?;

        // create a tombstone record
        let tombstone_guid = Guid::random();
        insert_tombstone_record(&tx, tombstone_guid.to_string())?;

        // create sync metadata
        let global_guid = Guid::new("AAAA");
        let coll_guid = Guid::new("AAAA");
        let ids = CollSyncIds {
            global: global_guid.clone(),
            coll: coll_guid.clone(),
        };
        put_meta(&tx, GLOBAL_SYNCID_META_KEY, &ids.global)?;
        put_meta(&tx, COLLECTION_SYNCID_META_KEY, &ids.coll)?;

        // call reset for sign out
        reset_in_tx(&tx, &EngineSyncAssociation::Disconnected)?;

        // check that sync change counter has been reset
        let reset_record_exists: bool = tx.query_row(
            "SELECT EXISTS (
                SELECT 1
                FROM addresses_data
                WHERE sync_change_counter = 1
            )",
            NO_PARAMS,
            |row| row.get(0),
        )?;
        assert!(reset_record_exists);

        // check that the mirror and tombstone tables have no records
        assert!(get_all(&tx, "addresses_mirror".to_string())?.is_empty());
        assert!(get_all(&tx, "addresses_tombstones".to_string())?.is_empty());

        // check that the last sync time was reset to 0
        let expected_sync_time = 0;
        assert_eq!(
            get_meta::<i64>(&tx, LAST_SYNC_META_KEY)?.unwrap_or(1),
            expected_sync_time
        );

        // check that the meta records were deleted
        assert!(get_meta::<String>(&tx, GLOBAL_SYNCID_META_KEY)?.is_none());
        assert!(get_meta::<String>(&tx, COLLECTION_SYNCID_META_KEY)?.is_none());

        clear_tables(&tx)?;

        // re-populating the tables
        insert_record(&tx, &address)?;
        insert_mirror_record(&tx, &mirror_record)?;
        insert_tombstone_record(&tx, tombstone_guid.to_string())?;

        // call reset for sign in
        reset_in_tx(&tx, &EngineSyncAssociation::Connected(ids))?;

        // check that the meta records were set
        let retrieved_global_sync_id = get_meta::<String>(&tx, GLOBAL_SYNCID_META_KEY)?;
        assert_eq!(
            retrieved_global_sync_id.unwrap_or_default(),
            global_guid.to_string()
        );

        let retrieved_coll_sync_id = get_meta::<String>(&tx, COLLECTION_SYNCID_META_KEY)?;
        assert_eq!(
            retrieved_coll_sync_id.unwrap_or_default(),
            coll_guid.to_string()
        );

        Ok(())
    }
}

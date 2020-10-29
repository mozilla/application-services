/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::models::address::{Address, InternalAddress, NewAddressFields};
use crate::db::schema::ADDRESS_COMMON_COLS;
use crate::error::*;

use rusqlite::{Connection, NO_PARAMS};
use sync_guid::Guid;
use types::Timestamp;

#[allow(dead_code)]
pub fn add_address(conn: &Connection, new_address: NewAddressFields) -> Result<InternalAddress> {
    let tx = conn.unchecked_transaction()?;

    let address = InternalAddress {
        guid: Guid::random(),
        fields: new_address,
        time_created: Timestamp::now(),
        time_last_used: Some(Timestamp::now()),
        time_last_modified: Timestamp::now(),
        times_used: 0,
        sync_change_counter: 1,
    };

    tx.execute_named(
        &format!(
            "INSERT OR IGNORE INTO addresses_data (
                {common_cols}
            ) VALUES (
                :guid,
                :given_name,
                :additional_name,
                :family_name,
                :organization,
                :street_address,
                :address_level3,
                :address_level2,
                :address_level1,
                :postal_code,
                :country,
                :tel,
                :email,
                :time_created,
                :time_last_used,
                :time_last_modified,
                :times_used,
                :sync_change_counter
            )",
            common_cols = ADDRESS_COMMON_COLS
        ),
        rusqlite::named_params! {
            ":guid": address.guid,
            ":given_name": address.fields.given_name,
            ":additional_name": address.fields.additional_name,
            ":family_name": address.fields.family_name,
            ":organization": address.fields.organization,
            ":street_address": address.fields.street_address,
            ":address_level3": address.fields.address_level3,
            ":address_level2": address.fields.address_level2,
            ":address_level1": address.fields.address_level1,
            ":postal_code": address.fields.postal_code,
            ":country": address.fields.country,
            ":tel": address.fields.tel,
            ":email": address.fields.email,
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
pub fn get_address(conn: &Connection, guid: String) -> Result<InternalAddress> {
    let tx = conn.unchecked_transaction()?;
    let sql = format!(
        "SELECT
            {common_cols}
        FROM addresses_data
        WHERE guid = :guid",
        common_cols = ADDRESS_COMMON_COLS
    );

    let address = tx.query_row(&sql, &[guid.as_str()], |row| {
        Ok(InternalAddress::from_row(row)?)
    })?;

    tx.commit()?;
    Ok(address)
}

#[allow(dead_code)]
pub fn get_all_addresses(conn: &Connection) -> Result<Vec<InternalAddress>> {
    let tx = conn.unchecked_transaction()?;
    let mut addresses = Vec::new();
    let sql = format!(
        "SELECT
            {common_cols}
        FROM addresses_data",
        common_cols = ADDRESS_COMMON_COLS
    );

    {
        let mut stmt = tx.prepare(&sql)?;
        let addresses_iter =
            stmt.query_map(NO_PARAMS, |row| Ok(InternalAddress::from_row(row)?))?;

        for address_result in addresses_iter {
            addresses.push(address_result.expect("Should unwrap address"));
        }
    }

    tx.commit()?;
    Ok(addresses)
}

#[allow(dead_code)]
pub fn update_address(conn: &Connection, address: &Address) -> Result<()> {
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
            ":guid": address.guid,
        },
    )?;

    tx.commit()?;
    Ok(())
}

pub fn delete_address(conn: &Connection, guid: String) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;

    // check that guid exists
    let exists = tx.query_row(
        "SELECT EXISTS (
            SELECT 1
            FROM addresses_data d
            WHERE guid = :guid
                AND NOT EXISTS (
                    SELECT 1
                    FROM   addresses_tombstones t
                    WHERE  d.guid = t.guid
                )
        )",
        &[guid.as_str()],
        |row| row.get(0),
    )?;

    if exists {
        tx.execute_named(
            "DELETE FROM addresses_data
            WHERE guid = :guid",
            rusqlite::named_params! {
                ":guid": guid.as_str(),
            },
        )?;

        tx.execute_named(
            "INSERT OR IGNORE INTO addresses_tombstones (
                guid,
                time_deleted
            ) VALUES (
                :guid,
                :time_deleted
            )",
            rusqlite::named_params! {
                ":guid": guid.as_str(),
                ":time_deleted": Timestamp::now(),
            },
        )?;
    }

    tx.commit()?;
    Ok(exists)
}

pub fn touch(conn: &Connection, guid: String) -> Result<()> {
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
            ":guid": guid.as_str(),
        },
    )?;

    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test::new_mem_db;

    #[test]
    fn test_address_create_and_read() {
        let mut db = new_mem_db();

        let saved_address = add_address(
            &mut db,
            NewAddressFields {
                given_name: "jane".to_string(),
                family_name: "doe".to_string(),
                street_address: "123 Main Street".to_string(),
                address_level2: "Seattle, WA".to_string(),
                country: "United States".to_string(),

                ..NewAddressFields::default()
            },
        )
        .expect("should contain saved address");

        // check that the add function populated the guid field
        assert_ne!(Guid::default(), saved_address.guid);

        // check that sync_change_counter was set
        assert_eq!(1, saved_address.sync_change_counter);

        // get created address
        let retrieved_address = get_address(&mut db, saved_address.guid.to_string())
            .expect("should contain optional retrieved address");
        assert_eq!(saved_address.guid, retrieved_address.guid);
        assert_eq!(
            saved_address.fields.given_name,
            retrieved_address.fields.given_name
        );
        assert_eq!(
            saved_address.fields.family_name,
            retrieved_address.fields.family_name
        );
        assert_eq!(
            saved_address.fields.street_address,
            retrieved_address.fields.street_address
        );
        assert_eq!(
            saved_address.fields.address_level2,
            retrieved_address.fields.address_level2
        );
        assert_eq!(
            saved_address.fields.country,
            retrieved_address.fields.country
        );

        // converting the created record into a tombstone to check that it's not returned on a second `get_address` call
        let delete_result = delete_address(&mut db, saved_address.guid.to_string());
        assert!(delete_result.is_ok());
        assert!(delete_result.unwrap());

        assert!(get_address(&mut db, saved_address.guid.to_string()).is_err());
    }

    #[test]
    fn test_address_read_all() {
        let mut db = new_mem_db();

        let saved_address = add_address(
            &mut db,
            NewAddressFields {
                given_name: "jane".to_string(),
                family_name: "doe".to_string(),
                street_address: "123 Second Avenue".to_string(),
                address_level2: "Chicago, IL".to_string(),
                country: "United States".to_string(),

                ..NewAddressFields::default()
            },
        )
        .expect("should contain saved address");

        let saved_address2 = add_address(
            &mut db,
            NewAddressFields {
                given_name: "john".to_string(),
                family_name: "deer".to_string(),
                street_address: "123 First Avenue".to_string(),
                address_level2: "Los Angeles, CA".to_string(),
                country: "United States".to_string(),

                ..NewAddressFields::default()
            },
        )
        .expect("should contain saved address");

        // creating a third address with a tombstone to ensure it's not retunred
        let saved_address3 = add_address(
            &mut db,
            NewAddressFields {
                given_name: "abraham".to_string(),
                family_name: "lincoln".to_string(),
                street_address: "1600 Pennsylvania Ave NW".to_string(),
                address_level2: "Washington, DC".to_string(),
                country: "United States".to_string(),

                ..NewAddressFields::default()
            },
        )
        .expect("should contain saved address");

        let delete_result = delete_address(&mut db, saved_address3.guid.to_string());
        assert!(delete_result.is_ok());
        assert!(delete_result.unwrap());

        let retrieved_addresses =
            get_all_addresses(&mut db).expect("Should contain all saved addresses");

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
        let mut db = new_mem_db();

        let saved_address = add_address(
            &mut db,
            NewAddressFields {
                given_name: "john".to_string(),
                family_name: "doe".to_string(),
                street_address: "1300 Broadway".to_string(),
                address_level2: "New York, NY".to_string(),
                country: "United States".to_string(),

                ..NewAddressFields::default()
            },
        )
        .expect("should contain saved address");

        let expected_additional_name = "paul".to_string();
        let update_result = update_address(
            &mut db,
            &Address {
                guid: saved_address.guid.to_string(),
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

        let updated_address = get_address(&mut db, saved_address.guid.to_string())
            .expect("should contain optional updated address");

        assert_eq!(saved_address.guid, updated_address.guid);
        assert_eq!(
            expected_additional_name,
            updated_address.fields.additional_name
        );

        //check that the sync_change_counter was incremented
        assert_eq!(2, updated_address.sync_change_counter);
    }

    #[test]
    fn test_address_delete() {
        let mut db = new_mem_db();

        let saved_address = add_address(
            &mut db,
            NewAddressFields {
                given_name: "jane".to_string(),
                family_name: "doe".to_string(),
                street_address: "123 Second Avenue".to_string(),
                address_level2: "Chicago, IL".to_string(),
                country: "United States".to_string(),

                ..NewAddressFields::default()
            },
        )
        .expect("should contain saved address");

        let delete_result = delete_address(&mut db, saved_address.guid.to_string());
        assert!(delete_result.is_ok());
        assert!(delete_result.unwrap());
    }

    #[test]
    fn test_address_trigger_on_create() {
        let db = new_mem_db();
        let guid = Guid::random();

        // create a tombstone record
        let tombstone_result = db.execute_named(
            "INSERT OR IGNORE INTO addresses_tombstones (
                guid,
                time_deleted
            ) VALUES (
                :guid,
                :time_deleted
            )",
            rusqlite::named_params! {
                ":guid": guid.as_str(),
                ":time_deleted": Timestamp::now(),
            },
        );
        assert!(tombstone_result.is_ok());

        // create a new address with the tombstone's guid
        let address = InternalAddress {
            guid,
            fields: NewAddressFields {
                given_name: "jane".to_string(),
                family_name: "doe".to_string(),
                street_address: "123 Second Avenue".to_string(),
                address_level2: "Chicago, IL".to_string(),
                country: "United States".to_string(),

                ..NewAddressFields::default()
            },

            ..InternalAddress::default()
        };

        let add_address_result = db.execute_named(
            &format!(
                "INSERT OR IGNORE INTO addresses_data (
                    {common_cols}
                ) VALUES (
                    :guid,
                    :given_name,
                    :additional_name,
                    :family_name,
                    :organization,
                    :street_address,
                    :address_level3,
                    :address_level2,
                    :address_level1,
                    :postal_code,
                    :country,
                    :tel,
                    :email,
                    :time_created,
                    :time_last_used,
                    :time_last_modified,
                    :times_used,
                    :sync_change_counter
                )",
                common_cols = ADDRESS_COMMON_COLS
            ),
            rusqlite::named_params! {
                ":guid": address.guid,
                ":given_name": address.fields.given_name,
                ":additional_name": address.fields.additional_name,
                ":family_name": address.fields.family_name,
                ":organization": address.fields.organization,
                ":street_address": address.fields.street_address,
                ":address_level3": address.fields.address_level3,
                ":address_level2": address.fields.address_level2,
                ":address_level1": address.fields.address_level1,
                ":postal_code": address.fields.postal_code,
                ":country": address.fields.country,
                ":tel": address.fields.tel,
                ":email": address.fields.email,
                ":time_created": address.time_created,
                ":time_last_used": address.time_last_used,
                ":time_last_modified": address.time_last_modified,
                ":times_used": address.times_used,
                ":sync_change_counter": address.sync_change_counter,
            },
        );
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
            fields: NewAddressFields {
                given_name: "jane".to_string(),
                family_name: "doe".to_string(),
                street_address: "123 Second Avenue".to_string(),
                address_level2: "Chicago, IL".to_string(),
                country: "United States".to_string(),

                ..NewAddressFields::default()
            },

            ..InternalAddress::default()
        };

        let add_address_result = db.execute_named(
            &format!(
                "INSERT OR IGNORE INTO addresses_data (
                    {common_cols}
                ) VALUES (
                    :guid,
                    :given_name,
                    :additional_name,
                    :family_name,
                    :organization,
                    :street_address,
                    :address_level3,
                    :address_level2,
                    :address_level1,
                    :postal_code,
                    :country,
                    :tel,
                    :email,
                    :time_created,
                    :time_last_used,
                    :time_last_modified,
                    :times_used,
                    :sync_change_counter
                )",
                common_cols = ADDRESS_COMMON_COLS
            ),
            rusqlite::named_params! {
                ":guid": address.guid,
                ":given_name": address.fields.given_name,
                ":additional_name": address.fields.additional_name,
                ":family_name": address.fields.family_name,
                ":organization": address.fields.organization,
                ":street_address": address.fields.street_address,
                ":address_level3": address.fields.address_level3,
                ":address_level2": address.fields.address_level2,
                ":address_level1": address.fields.address_level1,
                ":postal_code": address.fields.postal_code,
                ":country": address.fields.country,
                ":tel": address.fields.tel,
                ":email": address.fields.email,
                ":time_created": address.time_created,
                ":time_last_used": address.time_last_used,
                ":time_last_modified": address.time_last_modified,
                ":times_used": address.times_used,
                ":sync_change_counter": address.sync_change_counter,
            },
        );
        assert!(add_address_result.is_ok());

        // create a tombstone record with the same guid
        let tombstone_result = db.execute_named(
            "INSERT OR IGNORE INTO addresses_tombstones (
                guid,
                time_deleted
            ) VALUES (
                :guid,
                :time_deleted
            )",
            rusqlite::named_params! {
                ":guid": address.guid.as_str(),
                ":time_deleted": Timestamp::now(),
            },
        );
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
            NewAddressFields {
                given_name: "jane".to_string(),
                family_name: "doe".to_string(),
                street_address: "123 Second Avenue".to_string(),
                address_level2: "Chicago, IL".to_string(),
                country: "United States".to_string(),

                ..NewAddressFields::default()
            },
        )?;

        assert_eq!(saved_address.sync_change_counter, 1);
        assert_eq!(saved_address.times_used, 0);

        touch(&db, saved_address.guid.to_string())?;

        let touched_address = get_address(&db, saved_address.guid.to_string())?;

        assert_eq!(touched_address.sync_change_counter, 2);
        assert_eq!(touched_address.times_used, 1);

        Ok(())
    }
}

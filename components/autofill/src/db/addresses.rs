/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::{
    models::{
        address::{
            AddressMeta, InternalAddress, UpdatableAddressFields, UpdatableAddressFieldsWithMeta,
        },
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

/// Adds an address **including metadata**, taking the guid, timestamps and sync
/// change counter from the caller rather than generating them. Normally you will
/// use `add_address` instead; this is for importing records from another store
/// that already have metadata.
pub(crate) fn add_address_with_meta(
    conn: &Connection,
    fields: UpdatableAddressFields,
    meta: AddressMeta,
) -> Result<InternalAddress> {
    let tx = conn.unchecked_transaction()?;
    let address = internal_address_from_meta(fields, &meta);
    add_internal_address(&tx, &address)?;
    tx.commit()?;
    Ok(address)
}

/// Adds multiple addresses **including metadata** within a single transaction.
/// Each record gets its own result, so a record that fails to insert is reported
/// as `Err(message)` without aborting the rest of the batch.
pub(crate) fn add_many_addresses_with_meta(
    conn: &Connection,
    entries: Vec<UpdatableAddressFieldsWithMeta>,
) -> Result<Vec<std::result::Result<InternalAddress, String>>> {
    let tx = conn.unchecked_transaction()?;
    let mut results = Vec::with_capacity(entries.len());
    for entry in entries {
        let address = internal_address_from_meta(entry.fields, &entry.meta);
        match with_savepoint(&tx, || add_internal_address(&tx, &address))? {
            Ok(()) => results.push(Ok(address)),
            Err(e) => results.push(Err(e.to_string())),
        }
    }
    tx.commit()?;
    Ok(results)
}

/// Runs `op` in a savepoint, rolling back to it if `op` fails, so that a record
/// reported as an error by the bulk functions leaves nothing behind. The shared
/// triggers reject a guid that exists in the counterpart table with
/// `RAISE(FAIL)`, which aborts the statement but keeps the row it already
/// inserted - so without this the offending row would be committed along with
/// the rest of the batch, putting the guid in both `addresses_data` and
/// `addresses_tombstones`.
///
/// The outer `Result` is a savepoint failure and aborts the batch; the inner one
/// is the record's own failure.
fn with_savepoint<T>(
    tx: &Transaction<'_>,
    op: impl FnOnce() -> Result<T>,
) -> Result<std::result::Result<T, Error>> {
    tx.execute_batch("SAVEPOINT bulk_record")?;
    match op() {
        Ok(value) => {
            tx.execute_batch("RELEASE bulk_record")?;
            Ok(Ok(value))
        }
        Err(e) => {
            tx.execute_batch("ROLLBACK TO bulk_record; RELEASE bulk_record")?;
            Ok(Err(e))
        }
    }
}

/// Removes every address and every address tombstone, in one transaction.
///
/// Deleting the rows alone is not enough. A delete leaves a tombstone behind for
/// any guid the sync mirror knows, and the insert trigger then rejects re-adding
/// that guid, so a wipe that kept them could not be followed by a re-import of
/// the same records. Clearing both tables is what makes the wipe repeatable.
pub(crate) fn delete_all_addresses(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM addresses_data", [])?;
    // After the data, so the tombstones the delete trigger just created go too.
    tx.execute("DELETE FROM addresses_tombstones", [])?;
    tx.commit()?;
    Ok(())
}

/// Adds tombstones for records that were deleted locally but not yet uploaded,
/// within a single transaction and with a result per record. `time_deleted` comes
/// from the caller rather than being stamped as now, so that a deletion imported
/// from another store keeps its original time. Without the tombstone the next
/// sync has nothing to say the record was deleted and takes the server copy.
pub(crate) fn add_many_address_tombstones(
    conn: &Connection,
    tombstones: Vec<(String, i64)>,
) -> Result<Vec<std::result::Result<String, String>>> {
    let tx = conn.unchecked_transaction()?;
    let mut results = Vec::with_capacity(tombstones.len());
    for (guid, time_deleted) in tombstones {
        let inserted = with_savepoint(&tx, || {
            tx.execute(
                "INSERT INTO addresses_tombstones (guid, time_deleted)
                 VALUES (:guid, :time_deleted)",
                rusqlite::named_params! {
                    ":guid": &guid,
                    ":time_deleted": timestamp_from_millis(time_deleted),
                },
            )?;
            Ok(())
        })?;
        match inserted {
            Ok(()) => results.push(Ok(guid)),
            Err(e) => results.push(Err(e.to_string())),
        }
    }
    tx.commit()?;
    Ok(results)
}

/// `Timestamp` is a `u64`, so a negative millisecond value would wrap to a huge
/// one and then win every "latest wins" comparison in `Metadata::merge`. Clamp to
/// 0, which already means "unset" for these fields. The tuple constructor is used
/// rather than `Timestamp::from`, which asserts non-zero.
fn timestamp_from_millis(millis: i64) -> Timestamp {
    Timestamp(millis.max(0) as u64)
}

fn internal_address_from_meta(
    fields: UpdatableAddressFields,
    meta: &AddressMeta,
) -> InternalAddress {
    InternalAddress {
        guid: Guid::new(&meta.guid),
        name: fields.name,
        organization: fields.organization,
        street_address: fields.street_address,
        address_level3: fields.address_level3,
        address_level2: fields.address_level2,
        address_level1: fields.address_level1,
        postal_code: fields.postal_code,
        country: fields.country,
        tel: fields.tel,
        email: fields.email,
        metadata: Metadata {
            time_created: timestamp_from_millis(meta.time_created),
            time_last_used: timestamp_from_millis(meta.time_last_used.unwrap_or(0)),
            time_last_modified: timestamp_from_millis(meta.time_last_modified),
            times_used: meta.times_used,
            sync_change_counter: meta.sync_change_counter,
        },
    }
}

/// Updates an address **including metadata**, setting both its fields and its
/// timestamps and `times_used` to the supplied values. Normally you will use
/// `update_address` instead, which owns the metadata itself; this is for keeping
/// a record identical to one held in another store. Errors with `NoSuchRecord`
/// if the guid is absent.
pub(crate) fn update_address_with_meta(
    conn: &Connection,
    fields: UpdatableAddressFields,
    meta: AddressMeta,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    let address = internal_address_from_meta(fields, &meta);
    // Checked up front because `update_internal_address` asserts on the number
    // of rows changed rather than returning an error.
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM addresses_data WHERE guid = :guid)",
        rusqlite::named_params! { ":guid": address.guid },
        |row| row.get(0),
    )?;
    if !exists {
        return Err(Error::NoSuchRecord(address.guid.to_string()));
    }
    update_internal_address(
        &tx,
        &address,
        CounterUpdate::Set(address.metadata.sync_change_counter),
    )?;
    tx.commit()?;
    Ok(())
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

pub(crate) fn count_all_addresses(conn: &Connection) -> Result<i64> {
    let sql = "SELECT COUNT(*)
        FROM addresses_data";

    let mut stmt = conn.prepare(sql)?;
    let count: i64 = stmt.query_row([], |row| row.get(0))?;
    Ok(count)
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

/// How `update_internal_address` should treat the change counter.
pub(crate) enum CounterUpdate {
    /// Record a local change awaiting upload.
    Increment,
    /// Leave the counter alone, for a change that must not be uploaded - eg one
    /// applied by Sync, which is already what the server has.
    Leave,
    /// Replace the counter, for a record whose counter is owned by the caller.
    Set(i64),
}

impl CounterUpdate {
    /// The SQL assigned to `sync_change_counter`, and the value bound to
    /// `:counter` within it. `Leave` adds 0 rather than dropping `:counter` from
    /// the SQL, because rusqlite rejects a named parameter the statement doesn't
    /// use.
    fn as_sql(&self) -> (&'static str, i64) {
        match self {
            Self::Increment => ("sync_change_counter + :counter", 1),
            Self::Leave => ("sync_change_counter + :counter", 0),
            Self::Set(counter) => (":counter", *counter),
        }
    }
}

/// Updates all fields including metadata - although the change counter gets
/// slightly special treatment, see `CounterUpdate`.
pub(crate) fn update_internal_address(
    tx: &Transaction<'_>,
    address: &InternalAddress,
    counter: CounterUpdate,
) -> Result<()> {
    let (counter_sql, counter_value) = counter.as_sql();
    let rows_changed = tx.execute(
        &format!(
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
            sync_change_counter = {counter_sql}
        WHERE guid              = :guid"
        ),
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
            ":counter": counter_value,
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

        let address_count = count_all_addresses(&db).expect("Should count all saved addresses");
        assert_eq!(expected_number_of_addresses, address_count as usize);

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
            CounterUpdate::Leave,
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

    fn test_fields(street_address: &str) -> UpdatableAddressFields {
        UpdatableAddressFields {
            name: "jane doe".to_string(),
            street_address: street_address.to_string(),
            address_level2: "Seattle, WA".to_string(),
            country: "United States".to_string(),
            ..UpdatableAddressFields::default()
        }
    }

    fn test_meta(guid: &str, sync_change_counter: i64) -> AddressMeta {
        AddressMeta {
            guid: guid.to_string(),
            time_created: 1000,
            time_last_used: Some(2000),
            time_last_modified: 3000,
            times_used: 4,
            sync_change_counter,
        }
    }

    #[test]
    fn test_address_add_with_meta() -> Result<()> {
        let db = new_mem_db();

        let saved =
            add_address_with_meta(&db, test_fields("123 Main Street"), test_meta("abc", 2))?;

        // the supplied guid is used rather than a fresh one being generated.
        assert_eq!(saved.guid.as_str(), "abc");

        let retrieved = get_address(&db, &Guid::new("abc"))?;
        assert_eq!(retrieved.street_address, "123 Main Street");
        assert_eq!(retrieved.metadata.time_created.as_millis(), 1000);
        assert_eq!(retrieved.metadata.time_last_used.as_millis(), 2000);
        assert_eq!(retrieved.metadata.time_last_modified.as_millis(), 3000);
        assert_eq!(retrieved.metadata.times_used, 4);
        assert_eq!(retrieved.metadata.sync_change_counter, 2);

        Ok(())
    }

    #[test]
    fn test_address_add_with_meta_clamps_negative_timestamps() -> Result<()> {
        let db = new_mem_db();

        let meta = AddressMeta {
            guid: "abc".to_string(),
            time_created: -1,
            time_last_used: Some(-1),
            time_last_modified: -1,
            times_used: 0,
            sync_change_counter: 0,
        };
        add_address_with_meta(&db, test_fields("123 Main Street"), meta)?;

        let retrieved = get_address(&db, &Guid::new("abc"))?;
        assert_eq!(retrieved.metadata.time_created.as_millis(), 0);
        assert_eq!(retrieved.metadata.time_last_used.as_millis(), 0);
        assert_eq!(retrieved.metadata.time_last_modified.as_millis(), 0);

        Ok(())
    }

    #[test]
    fn test_address_update_with_meta_keeps_supplied_counter() -> Result<()> {
        let db = new_mem_db();

        add_address_with_meta(&db, test_fields("123 Main Street"), test_meta("abc", 0))?;

        // the supplied counter must be applied, not the one already in the row.
        update_address_with_meta(&db, test_fields("456 Second Avenue"), test_meta("abc", 1))?;

        let retrieved = get_address(&db, &Guid::new("abc"))?;
        assert_eq!(retrieved.street_address, "456 Second Avenue");
        assert_eq!(retrieved.metadata.sync_change_counter, 1);

        // and back down again.
        update_address_with_meta(&db, test_fields("456 Second Avenue"), test_meta("abc", 0))?;
        assert_eq!(
            get_address(&db, &Guid::new("abc"))?
                .metadata
                .sync_change_counter,
            0
        );

        Ok(())
    }

    #[test]
    fn test_address_update_with_meta_errors_when_missing() -> Result<()> {
        let db = new_mem_db();

        let result =
            update_address_with_meta(&db, test_fields("123 Main Street"), test_meta("abc", 3));
        assert!(matches!(result, Err(Error::NoSuchRecord(guid)) if guid == "abc"));
        assert!(get_address(&db, &Guid::new("abc")).is_err());

        Ok(())
    }

    #[test]
    fn test_address_add_many_with_meta_isolates_failures() -> Result<()> {
        let db = new_mem_db();

        // the second entry has an empty guid, which the `addresses_data` CHECK
        // constraint rejects. The others must still be inserted.
        let results = add_many_addresses_with_meta(
            &db,
            vec![
                UpdatableAddressFieldsWithMeta {
                    fields: test_fields("1 First Street"),
                    meta: test_meta("aaa", 1),
                },
                UpdatableAddressFieldsWithMeta {
                    fields: test_fields("2 Second Street"),
                    meta: test_meta("", 1),
                },
                UpdatableAddressFieldsWithMeta {
                    fields: test_fields("3 Third Street"),
                    meta: test_meta("ccc", 1),
                },
            ],
        )?;

        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
        assert!(results[2].is_ok());

        assert_eq!(get_all_addresses(&db)?.len(), 2);
        assert_eq!(get_address(&db, &Guid::new("aaa"))?.metadata.times_used, 4);

        Ok(())
    }

    #[test]
    fn test_delete_all_addresses_allows_a_reimport() -> Result<()> {
        let db = new_mem_db();

        // A tombstone left by an earlier import, and a record sharing no guid
        // with it.
        add_many_address_tombstones(&db, vec![("gone".to_string(), 1234)])?;
        let address = add_address(&db, UpdatableAddressFields::default())?;

        delete_all_addresses(&db)?;
        assert_eq!(get_all_addresses(&db)?.len(), 0);
        let tombstones: i64 =
            db.query_row("SELECT COUNT(*) FROM addresses_tombstones", [], |row| {
                row.get(0)
            })?;
        assert_eq!(tombstones, 0, "tombstones are cleared with the records");

        // The point of clearing them: re-importing the same guids succeeds,
        // where the insert trigger would reject a guid still tombstoned.
        let results = add_many_addresses_with_meta(
            &db,
            vec![
                UpdatableAddressFieldsWithMeta {
                    fields: UpdatableAddressFields::default(),
                    meta: AddressMeta {
                        guid: address.guid.to_string(),
                        ..Default::default()
                    },
                },
                UpdatableAddressFieldsWithMeta {
                    fields: UpdatableAddressFields::default(),
                    meta: AddressMeta {
                        guid: "gone".to_string(),
                        ..Default::default()
                    },
                },
            ],
        )?;
        assert!(
            results.iter().all(|r| r.is_ok()),
            "a previously tombstoned guid can be re-imported: {results:?}"
        );

        Ok(())
    }

    #[test]
    fn test_address_add_many_tombstones() -> Result<()> {
        let db = new_mem_db();

        let results = add_many_address_tombstones(&db, vec![("aaa".to_string(), 1234)])?;
        assert_eq!(results.len(), 1);
        assert!(results[0].is_ok());

        // the supplied deletion time is used rather than being stamped as now.
        let time_deleted: i64 = db.query_row(
            "SELECT time_deleted FROM addresses_tombstones WHERE guid = 'aaa'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(time_deleted, 1234);

        Ok(())
    }

    #[test]
    fn test_address_add_many_tombstones_rejects_live_guid() -> Result<()> {
        let db = new_mem_db();

        add_address_with_meta(&db, test_fields("123 Main Street"), test_meta("abc", 0))?;

        // a guid cannot be in both `addresses_data` and `addresses_tombstones`;
        // the trigger enforcing that must not take the rest of the batch down.
        let results = add_many_address_tombstones(
            &db,
            vec![("abc".to_string(), 1234), ("ddd".to_string(), 5678)],
        )?;

        assert_eq!(results.len(), 2);
        assert!(results[0].is_err());
        assert!(results[1].is_ok());

        // the rejected tombstone must not have been committed anyway - see
        // `with_savepoint`.
        assert_eq!(count_tombstones(&db, "abc")?, 0);
        assert!(get_address(&db, &Guid::new("abc")).is_ok());
        assert_eq!(count_tombstones(&db, "ddd")?, 1);

        Ok(())
    }

    #[test]
    fn test_address_add_many_with_meta_rejects_deleted_guid() -> Result<()> {
        let db = new_mem_db();

        add_many_address_tombstones(&db, vec![("aaa".to_string(), 1234)])?;

        // the other side of the same invariant: a guid in
        // `addresses_tombstones` cannot be inserted into `addresses_data`.
        let results = add_many_addresses_with_meta(
            &db,
            vec![
                UpdatableAddressFieldsWithMeta {
                    fields: test_fields("1 First Street"),
                    meta: test_meta("aaa", 1),
                },
                UpdatableAddressFieldsWithMeta {
                    fields: test_fields("2 Second Street"),
                    meta: test_meta("bbb", 1),
                },
            ],
        )?;

        assert_eq!(results.len(), 2);
        assert!(results[0].is_err());
        assert!(results[1].is_ok());

        assert!(get_address(&db, &Guid::new("aaa")).is_err());
        assert_eq!(get_all_addresses(&db)?.len(), 1);

        Ok(())
    }

    fn count_tombstones(conn: &Connection, guid: &str) -> Result<i64> {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM addresses_tombstones WHERE guid = :guid",
            rusqlite::named_params! { ":guid": guid },
            |row| row.get(0),
        )?)
    }
}

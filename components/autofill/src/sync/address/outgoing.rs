/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::models::address::InternalAddress;
use crate::db::schema::ADDRESS_COMMON_COLS;
use crate::error::*;
use crate::sync::{address::AddressPayload, common::*};
use crate::sync::{CollectionName, OutgoingBso, OutgoingChangeset, ProcessOutgoingRecordImpl};
use rusqlite::{Row, Transaction};
use sync_guid::Guid as SyncGuid;

const DATA_TABLE_NAME: &str = "addresses_data";
const MIRROR_TABLE_NAME: &str = "addresses_mirror";
const STAGING_TABLE_NAME: &str = "addresses_sync_outgoing_staging";

pub(super) struct OutgoingAddressesImpl {}

impl ProcessOutgoingRecordImpl for OutgoingAddressesImpl {
    type Record = InternalAddress;

    /// Gets the local records that have unsynced changes or don't have corresponding mirror
    /// records and upserts them to the mirror table
    fn fetch_outgoing_records(
        &self,
        tx: &Transaction<'_>,
        collection_name: CollectionName,
    ) -> anyhow::Result<OutgoingChangeset> {
        // We left join the mirror table since we'll need to know if
        // there were any unknown fields from the server we need to roundtrip
        let data_sql = format!(
            "SELECT
                l.{common_cols},
                m.payload,
                l.sync_change_counter
            FROM addresses_data l
            LEFT JOIN addresses_mirror m
            ON l.guid = m.guid
            WHERE sync_change_counter > 0
                OR l.guid NOT IN (
                    SELECT m.guid
                    FROM addresses_mirror m
                )",
            common_cols = ADDRESS_COMMON_COLS,
        );
        let record_from_data_row: &dyn Fn(&Row<'_>) -> Result<(OutgoingBso, i64)> = &|row| {
            let mut record = InternalAddress::from_row(row)?.into_payload()?;
            let mirror_str = row.get::<_, Option<String>>("payload")?;
            // If the server had an unknown field we fetch it and add it to the record
            // we'll be uploading
            let mirror_payload: AddressPayload = match mirror_str {
                Some(s) => serde_json::from_str(&s)?,
                None => Default::default(),
            };
            record.entry.unknown_fields = mirror_payload.entry.unknown_fields;

            Ok((
                OutgoingBso::from_content_with_id(record)?,
                row.get::<_, i64>("sync_change_counter")?,
            ))
        };

        let tombstones_sql = "SELECT guid FROM addresses_tombstones";

        // save outgoing records to the mirror table.
        // unlike credit-cards, which stores records encrypted as they are
        // on the server to protect the sensitive fields, we just store the
        // plaintext payload.
        let staging_records = common_get_outgoing_staging_records(
            tx,
            &data_sql,
            tombstones_sql,
            record_from_data_row,
        )?
        .into_iter()
        .map(|(bso, change_counter)| (bso.envelope.id, bso.payload, change_counter))
        .collect::<Vec<_>>();
        common_save_outgoing_records(tx, STAGING_TABLE_NAME, staging_records)?;

        // return outgoing changes
        let outgoing_records =
            common_get_outgoing_records(tx, &data_sql, tombstones_sql, record_from_data_row)?
                .into_iter()
                .map(|(bso, _change_counter)| bso)
                .collect();
        Ok(OutgoingChangeset::new(collection_name, outgoing_records))
    }

    fn finish_synced_items(
        &self,
        tx: &Transaction<'_>,
        records_synced: Vec<SyncGuid>,
    ) -> anyhow::Result<()> {
        common_finish_synced_items(
            tx,
            DATA_TABLE_NAME,
            MIRROR_TABLE_NAME,
            STAGING_TABLE_NAME,
            records_synced,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{addresses::add_internal_address, models::address::InternalAddress};
    use crate::sync::{common::tests::*, test::new_syncable_mem_db, UnknownFields};
    use rusqlite::Connection;
    use serde_json::{json, Map, Value};
    use types::Timestamp;

    const COLLECTION_NAME: &str = "addresses";

    fn test_insert_mirror_record(
        conn: &Connection,
        address: InternalAddress,
        unknown_fields: UnknownFields,
    ) {
        // This should probably be in the sync module, but it's used here.
        let guid = address.guid.clone();
        let mut addr_payload = address.into_payload().unwrap();
        addr_payload.entry.unknown_fields = unknown_fields;
        let payload = serde_json::to_string(&addr_payload).expect("is json");
        conn.execute(
            "INSERT OR IGNORE INTO addresses_mirror (guid, payload)
             VALUES (:guid, :payload)",
            rusqlite::named_params! {
                ":guid": guid,
                ":payload": &payload,
            },
        )
        .expect("should insert");
    }

    lazy_static::lazy_static! {
        static ref TEST_JSON_RECORDS: Map<String, Value> = {
            // NOTE: the JSON here is the same as stored on the sync server -
            // the superfluous `entry` is unfortunate but from desktop.
            let val = json! {{
                "C" : {
                    "id": expand_test_guid('C'),
                    "entry": {
                        "givenName": "jane",
                        "familyName": "doe",
                        "streetAddress": "3050 South La Brea Ave",
                        "addressLevel2": "Los Angeles, CA",
                        "country": "United States",
                        "timeCreated": 0,
                        "timeLastUsed": 0,
                        "timeLastModified": 0,
                        "timesUsed": 0,
                        "version": 1,
                    }
                },
                "D" : {
                    "id": expand_test_guid('D'),
                    "entry": {
                        "given-name": "john",
                        "family-name": "doe",
                        "street-address": "85 Pike St",
                        "address-level2": "Seattle, WA",
                        "country": "United States",
                        "timeCreated": 0,
                        "timeLastUsed": 0,
                        "timeLastModified": 0,
                        "timesUsed": 0,
                        "foo": "bar",
                        "baz": "qux",
                        "version": 1,
                    }
                }
            }};
            val.as_object().expect("literal is an object").clone()
        };
    }

    fn test_json_record(guid_prefix: char) -> Value {
        TEST_JSON_RECORDS
            .get(&guid_prefix.to_string())
            .expect("should exist")
            .clone()
    }

    fn test_record(guid_prefix: char) -> InternalAddress {
        let json = test_json_record(guid_prefix);
        let payload = serde_json::from_value(json).unwrap();
        InternalAddress::from_payload(payload).expect("should be valid")
    }

    #[test]
    fn test_outgoing_never_synced() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ao = OutgoingAddressesImpl {};
        let test_record = test_record('C');

        // create data record
        assert!(add_internal_address(&tx, &test_record).is_ok());
        do_test_outgoing_never_synced(
            &tx,
            &ao,
            &test_record.guid,
            DATA_TABLE_NAME,
            MIRROR_TABLE_NAME,
            STAGING_TABLE_NAME,
            COLLECTION_NAME.into(),
        );
    }

    #[test]
    fn test_outgoing_tombstone() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ao = OutgoingAddressesImpl {};
        let test_record = test_record('C');

        // create tombstone record
        assert!(tx
            .execute(
                "INSERT INTO addresses_tombstones (
                    guid,
                    time_deleted
                ) VALUES (
                    :guid,
                    :time_deleted
                )",
                rusqlite::named_params! {
                    ":guid": test_record.guid,
                    ":time_deleted": Timestamp::now(),
                },
            )
            .is_ok());
        do_test_outgoing_tombstone(
            &tx,
            &ao,
            &test_record.guid,
            DATA_TABLE_NAME,
            MIRROR_TABLE_NAME,
            STAGING_TABLE_NAME,
            COLLECTION_NAME.into(),
        );
    }

    #[test]
    fn test_outgoing_synced_with_local_change() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ao = OutgoingAddressesImpl {};

        // create synced record with non-zero sync_change_counter
        let mut test_record = test_record('C');
        let initial_change_counter_val = 2;
        test_record.metadata.sync_change_counter = initial_change_counter_val;
        assert!(add_internal_address(&tx, &test_record).is_ok());
        test_insert_mirror_record(&tx, test_record.clone(), Default::default());
        exists_with_counter_value_in_table(
            &tx,
            DATA_TABLE_NAME,
            &test_record.guid,
            initial_change_counter_val,
        );

        do_test_outgoing_synced_with_local_change(
            &tx,
            &ao,
            &test_record.guid,
            DATA_TABLE_NAME,
            MIRROR_TABLE_NAME,
            STAGING_TABLE_NAME,
            COLLECTION_NAME.into(),
        );
    }

    #[test]
    fn test_outgoing_synced_with_no_change() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ao = OutgoingAddressesImpl {};

        // create synced record with no changes (sync_change_counter = 0)
        let test_record = test_record('C');
        assert!(add_internal_address(&tx, &test_record).is_ok());
        test_insert_mirror_record(&tx, test_record.clone(), Default::default());

        do_test_outgoing_synced_with_no_change(
            &tx,
            &ao,
            &test_record.guid,
            DATA_TABLE_NAME,
            STAGING_TABLE_NAME,
            COLLECTION_NAME.into(),
        );
    }

    #[test]
    fn test_outgoing_roundtrip_unknown() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ao = OutgoingAddressesImpl {};

        // create synced record with non-zero sync_change_counter
        let mut test_record = test_record('D');
        let initial_change_counter_val = 2;
        test_record.metadata.sync_change_counter = initial_change_counter_val;
        assert!(add_internal_address(&tx, &test_record).is_ok());
        // put "unknown_fields" into the mirror payload to imitate the server
        let unknown_fields: UnknownFields =
            serde_json::from_value(json! {{ "foo": "bar", "baz": "qux"}}).unwrap();
        test_insert_mirror_record(&tx, test_record.clone(), unknown_fields);
        exists_with_counter_value_in_table(
            &tx,
            DATA_TABLE_NAME,
            &test_record.guid,
            initial_change_counter_val,
        );

        let outgoing = &ao
            .fetch_outgoing_records(&tx, COLLECTION_NAME.into())
            .unwrap();
        // Kinda annoying to add every field here, probably a better way but i'll leave this for now
        let test_payload = json!({
            "id": "DDDDDDDDDDDD",
            "entry" : {
                "given-name": "john",
                "family-name": "doe",
                "street-address": "85 Pike St",
                "address-level2": "Seattle, WA",
                "country": "United States",
                "timeCreated": 0,
                "timeLastUsed": 0,
                "timeLastModified": 0,
                "timesUsed": 0,
                "foo": "bar",
                "baz": "qux",
                "version": 1,
                "organization": "",
                "postal-code": "",
                "email": "",
                "tel": "",
                "address-level1": "",
                "address-level3": "",
                "additional-name": "",
            }
        });
        assert_eq!(outgoing.changes[0].payload, test_payload.to_string());
        do_test_outgoing_synced_with_local_change(
            &tx,
            &ao,
            &test_record.guid,
            DATA_TABLE_NAME,
            MIRROR_TABLE_NAME,
            STAGING_TABLE_NAME,
            COLLECTION_NAME.into(),
        );
    }
}

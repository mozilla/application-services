/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::models::credit_card::InternalCreditCard;
use crate::db::schema::CREDIT_CARD_COMMON_COLS;
use crate::error::*;
use crate::sync::common::*;
use crate::sync::{
    OutgoingChangeset, Payload, ProcessOutgoingRecordImpl, ServerTimestamp, SyncRecord,
};
use rusqlite::{Row, Transaction};
use sync_guid::Guid as SyncGuid;

const DATA_TABLE_NAME: &str = "credit_cards_data";
const MIRROR_TABLE_NAME: &str = "credit_cards_mirror";
const STAGING_TABLE_NAME: &str = "credit_cards_sync_outgoing_staging";

pub(super) struct OutgoingCreditCardsImpl {}

impl ProcessOutgoingRecordImpl for OutgoingCreditCardsImpl {
    type Record = InternalCreditCard;

    /// Gets the local records that have unsynced changes or don't have corresponding mirror
    /// records and upserts them to the mirror table
    fn fetch_outgoing_records(
        &self,
        tx: &Transaction<'_>,
        collection_name: String,
        timestamp: ServerTimestamp,
    ) -> anyhow::Result<OutgoingChangeset> {
        let mut outgoing = OutgoingChangeset::new(collection_name, timestamp);

        let data_sql = format!(
            "SELECT
                {common_cols},
                sync_change_counter
            FROM credit_cards_data
            WHERE sync_change_counter > 0
                OR guid NOT IN (
                    SELECT m.guid
                    FROM credit_cards_mirror m
                )",
            common_cols = CREDIT_CARD_COMMON_COLS,
        );
        let payload_from_data_row: &dyn Fn(&Row<'_>) -> Result<Payload> =
            &|row| Ok(InternalCreditCard::from_row(row)?.to_payload()?);

        let tombstones_sql = "SELECT guid FROM credit_cards_tombstones";

        // save outgoing records to the mirror table
        let staging_records = common_get_outgoing_staging_records(
            &tx,
            &data_sql,
            &tombstones_sql,
            payload_from_data_row,
        )?;
        common_save_outgoing_records(&tx, STAGING_TABLE_NAME, staging_records)?;

        // return outgoing changes
        let outgoing_records: Vec<(Payload, i64)> =
            common_get_outgoing_records(&tx, &data_sql, &tombstones_sql, payload_from_data_row)?;

        outgoing.changes = outgoing_records
            .into_iter()
            .map(|(payload, _)| payload)
            .collect::<Vec<Payload>>();
        Ok(outgoing)
    }

    fn push_synced_items(
        &self,
        tx: &Transaction<'_>,
        records_synced: Vec<SyncGuid>,
    ) -> anyhow::Result<()> {
        common_push_synced_items(
            &tx,
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
    use crate::db::credit_cards::{add_internal_credit_card, tests::insert_mirror_record};
    use crate::sync::{common::tests::*, test::new_syncable_mem_db, SyncRecord};
    use serde_json::{json, Map, Value};
    use types::Timestamp;

    const COLLECTION_NAME: &str = "creditcards";

    lazy_static::lazy_static! {
        static ref TEST_JSON_RECORDS: Map<String, Value> = {
            // NOTE: the JSON here is the same as stored on the sync server -
            // the superfluous `entry` is unfortunate but from desktop.
            let val = json! {{
                "C" : {
                    "id": expand_test_guid('C'),
                    "entry": {
                        "cc-name": "Mr Me Another Person",
                        "cc-number": "87654321",
                        "cc-exp-month": 1,
                        "cc-exp-year": 2020,
                        "cc-type": "visa",
                        "timeCreated": 0,
                        "timeLastUsed": 0,
                        "timeLastModified": 0,
                        "timesUsed": 0,
                        "version": 3,
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

    fn test_record(guid_prefix: char) -> InternalCreditCard {
        let json = test_json_record(guid_prefix);
        let sync_payload = sync15::Payload::from_json(json).unwrap();
        InternalCreditCard::to_record(sync_payload).expect("should be valid")
    }

    #[test]
    fn test_outgoing_never_synced() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ao = OutgoingCreditCardsImpl {};
        let test_record = test_record('C');

        // create date record
        assert!(add_internal_credit_card(&tx, &test_record).is_ok());
        do_test_outgoing_never_synced(
            &tx,
            &ao,
            &test_record.guid,
            DATA_TABLE_NAME,
            MIRROR_TABLE_NAME,
            STAGING_TABLE_NAME,
            COLLECTION_NAME,
        );
    }

    #[test]
    fn test_outgoing_tombstone() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ao = OutgoingCreditCardsImpl {};
        let test_record = test_record('C');

        // create tombstone record
        assert!(tx
            .execute_named(
                "INSERT INTO credit_cards_tombstones (
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
            COLLECTION_NAME,
        );
    }

    #[test]
    fn test_outgoing_synced_with_local_change() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ao = OutgoingCreditCardsImpl {};

        // create synced record with non-zero sync_change_counter
        let mut test_record = test_record('C');
        let initial_change_counter_val = 2;
        test_record.metadata.sync_change_counter = initial_change_counter_val;
        assert!(add_internal_credit_card(&tx, &test_record).is_ok());
        insert_mirror_record(&tx, test_record.clone());
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
            COLLECTION_NAME,
        );
    }

    #[test]
    fn test_outgoing_synced_with_no_change() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ao = OutgoingCreditCardsImpl {};

        // create synced record with no changes (sync_change_counter = 0)
        let test_record = test_record('C');
        assert!(add_internal_credit_card(&tx, &test_record).is_ok());
        insert_mirror_record(&tx, test_record.clone());

        do_test_outgoing_synced_with_no_change(
            &tx,
            &ao,
            &test_record.guid,
            DATA_TABLE_NAME,
            STAGING_TABLE_NAME,
            COLLECTION_NAME,
        );
    }
}

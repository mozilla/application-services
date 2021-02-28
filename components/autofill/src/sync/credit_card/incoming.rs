/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::credit_cards::{add_internal_credit_card, update_internal_credit_card};
use crate::db::models::credit_card::InternalCreditCard;
use crate::db::schema::CREDIT_CARD_COMMON_COLS;
use crate::error::*;
use crate::sync::common::*;
use crate::sync::{
    IncomingRecord, IncomingState, LocalRecordInfo, Payload, ProcessIncomingRecordImpl,
    ServerTimestamp, SyncRecord,
};
use interrupt_support::Interruptee;
use rusqlite::{named_params, Transaction};
use sql_support::ConnExt;
use sync_guid::Guid as SyncGuid;

pub(super) struct IncomingCreditCardsImpl {}

impl ProcessIncomingRecordImpl for IncomingCreditCardsImpl {
    type Record = InternalCreditCard;

    /// The first step in the "apply incoming" process - stage the records
    fn stage_incoming(
        &self,
        tx: &Transaction<'_>,
        incoming: Vec<(Payload, ServerTimestamp)>,
        signal: &dyn Interruptee,
    ) -> Result<()> {
        common_stage_incoming_records(tx, "credit_cards_sync_staging", incoming, signal)
    }

    fn finish_incoming(&self, tx: &Transaction<'_>) -> Result<()> {
        common_mirror_staged_records(tx, "credit_cards_sync_staging", "credit_cards_mirror")
    }

    /// The second step in the "apply incoming" process for syncing autofill CC records.
    /// Incoming items are retrieved from the temp tables, deserialized, and
    /// assigned `IncomingState` values.
    fn fetch_incoming_states(
        &self,
        tx: &Transaction<'_>,
    ) -> Result<Vec<IncomingState<Self::Record>>> {
        let sql = "
        SELECT
            s.guid as guid,
            l.guid as l_guid,
            t.guid as t_guid,
            s.payload as s_payload,
            m.payload as m_payload,
            l.cc_name,
            l.cc_number,
            l.cc_exp_month,
            l.cc_exp_year,
            l.cc_type,
            l.time_created,
            l.time_last_used,
            l.time_last_modified,
            l.times_used,
            l.sync_change_counter
        FROM temp.credit_cards_sync_staging s
        LEFT JOIN credit_cards_mirror m ON s.guid = m.guid
        LEFT JOIN credit_cards_data l ON s.guid = l.guid
        LEFT JOIN credit_cards_tombstones t ON s.guid = t.guid";

        Ok(tx.query_rows_and_then_named(
            sql,
            &[],
            |row| -> Result<IncomingState<Self::Record>> {
                // the 'guid' and 's_payload' rows must be non-null.
                let guid: SyncGuid = row.get("guid")?;
                // the incoming sync15::Payload
                let incoming_payload =
                    Payload::from_json(serde_json::from_str(&row.get::<_, String>("s_payload")?)?)?;

                Ok(IncomingState {
                    incoming: {
                        if incoming_payload.is_tombstone() {
                            IncomingRecord::Tombstone {
                                guid: incoming_payload.id().into(),
                            }
                        } else {
                            IncomingRecord::Record {
                                record: InternalCreditCard::from_payload(incoming_payload)?,
                            }
                        }
                    },
                    local: match row.get_unwrap::<_, Option<String>>("l_guid") {
                        Some(l_guid) => {
                            assert_eq!(l_guid, guid);
                            // local record exists, check the state.
                            let record = InternalCreditCard::from_row(row)?;
                            let has_changes = record.metadata().sync_change_counter != 0;
                            if has_changes {
                                LocalRecordInfo::Modified { record }
                            } else {
                                LocalRecordInfo::Unmodified { record }
                            }
                        }
                        None => {
                            // no local record - maybe a tombstone?
                            match row.get::<_, Option<String>>("t_guid")? {
                                Some(t_guid) => {
                                    assert_eq!(guid, t_guid);
                                    LocalRecordInfo::Tombstone { guid }
                                }
                                None => LocalRecordInfo::Missing,
                            }
                        }
                    },
                    mirror: {
                        match row.get::<_, Option<String>>("m_payload")? {
                            Some(m_payload) => {
                                let payload =
                                    Payload::from_json(serde_json::from_str(&m_payload)?)?;
                                Some(InternalCreditCard::from_payload(payload)?)
                            }
                            None => None,
                        }
                    },
                })
            },
        )?)
    }

    /// Returns a local record that has the same values as the given incoming record (with the exception
    /// of the `guid` values which should differ) that will be used as a local duplicate record for
    /// syncing.
    fn get_local_dupe(
        &self,
        tx: &Transaction<'_>,
        incoming: &Self::Record,
    ) -> Result<Option<(SyncGuid, Self::Record)>> {
        let sql = format!("
            SELECT
                {common_cols},
                sync_change_counter
            FROM credit_cards_data
            WHERE
                -- `guid <> :guid` is a pre-condition for this being called, but...
                guid <> :guid
                -- only non-synced records are candidates, which means can't already be in the mirror.
                AND guid NOT IN (
                    SELECT guid
                    FROM credit_cards_mirror
                )
                -- and sql can check the field values.
                AND cc_name == :cc_name
                AND cc_number == :cc_number
                AND cc_exp_month == :cc_exp_month
                AND cc_exp_year == :cc_exp_year
                AND cc_type == :cc_type", common_cols = CREDIT_CARD_COMMON_COLS);

        let params = named_params! {
            ":guid": incoming.guid,
            ":cc_name": incoming.cc_name,
            ":cc_number": incoming.cc_number,
            ":cc_exp_month": incoming.cc_exp_month,
            ":cc_exp_year": incoming.cc_exp_year,
            ":cc_type": incoming.cc_type,
        };

        let result = tx.query_row_named(&sql, params, |row| {
            Ok(Self::Record::from_row(&row).expect("wtf? '?' doesn't work :("))
        });

        match result {
            Ok(r) => Ok(Some((incoming.guid.clone(), r))),
            Err(e) => match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                _ => Err(Error::SqlError(e)),
            },
        }
    }

    fn update_local_record(
        &self,
        tx: &Transaction<'_>,
        new_record: Self::Record,
        flag_as_changed: bool,
    ) -> Result<()> {
        update_internal_credit_card(tx, &new_record, flag_as_changed)?;
        Ok(())
    }

    fn insert_local_record(&self, tx: &Transaction<'_>, new_record: Self::Record) -> Result<()> {
        add_internal_credit_card(tx, &new_record)?;
        Ok(())
    }

    /// Changes the guid of the local record for the given `old_guid` to the given `new_guid` used
    /// for the `HasLocalDupe` incoming state, and mark the item as dirty.
    fn change_local_guid(
        &self,
        tx: &Transaction<'_>,
        old_guid: &SyncGuid,
        new_guid: &SyncGuid,
    ) -> Result<()> {
        common_change_guid(tx, "credit_cards_data", old_guid, new_guid)
    }

    fn remove_record(&self, tx: &Transaction<'_>, guid: &SyncGuid) -> Result<()> {
        common_remove_record(tx, "credit_cards_data", guid)
    }

    fn remove_tombstone(&self, tx: &Transaction<'_>, guid: &SyncGuid) -> Result<()> {
        common_remove_record(tx, "credit_cards_tombstones", guid)
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::test::new_syncable_mem_db;
    use super::*;
    use crate::db::credit_cards::get_credit_card;
    use crate::sync::common::tests::*;

    use interrupt_support::NeverInterrupts;
    use rusqlite::NO_PARAMS;
    use serde_json::{json, Map, Value};
    use sql_support::ConnExt;

    lazy_static::lazy_static! {
        static ref TEST_JSON_RECORDS: Map<String, Value> = {
            // NOTE: the JSON here is the same as stored on the sync server -
            // the superfluous `entry` is unfortunate but from desktop.
            let val = json! {{
                "A" : {
                    "id": expand_test_guid('A'),
                    "entry": {
                        "cc-name": "Mr Me A Person",
                        "cc-number": "12345678",
                        "cc-exp_month": 12,
                        "cc-exp_year": 2021,
                        "cc-type": "Cash!",
                        "version": 3,
                    }
                },
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
        InternalCreditCard::from_payload(sync_payload).expect("should be valid")
    }

    #[test]
    fn test_stage_incoming() -> Result<()> {
        let _ = env_logger::try_init();
        let mut db = new_syncable_mem_db();
        struct TestCase {
            incoming_records: Vec<Value>,
            expected_record_count: usize,
            expected_tombstone_count: usize,
        }

        let test_cases = vec![
            TestCase {
                incoming_records: vec![test_json_record('A')],
                expected_record_count: 1,
                expected_tombstone_count: 0,
            },
            TestCase {
                incoming_records: vec![test_json_tombstone('A')],
                expected_record_count: 0,
                expected_tombstone_count: 1,
            },
            TestCase {
                incoming_records: vec![
                    test_json_record('A'),
                    test_json_record('C'),
                    test_json_tombstone('B'),
                ],
                expected_record_count: 2,
                expected_tombstone_count: 1,
            },
        ];

        for tc in test_cases {
            log::info!("starting new testcase");
            let tx = db.transaction()?;
            let ri = IncomingCreditCardsImpl {};
            ri.stage_incoming(
                &tx,
                array_to_incoming(tc.incoming_records),
                &NeverInterrupts,
            )?;

            let payloads = tx.conn().query_rows_and_then_named(
                "SELECT * FROM temp.credit_cards_sync_staging;",
                &[],
                |row| -> Result<Payload> {
                    let payload: String = row.get_unwrap("payload");
                    Ok(Payload::from_json(serde_json::from_str(&payload)?)?)
                },
            )?;

            let record_count = payloads.iter().filter(|p| !p.is_tombstone()).count();
            let tombstone_count = payloads.len() - record_count;

            assert_eq!(record_count, tc.expected_record_count);
            assert_eq!(tombstone_count, tc.expected_tombstone_count);

            tx.execute("DELETE FROM temp.credit_cards_sync_staging;", NO_PARAMS)?;
        }
        Ok(())
    }

    #[test]
    fn test_change_local_guid() -> Result<()> {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction()?;
        let ri = IncomingCreditCardsImpl {};

        ri.insert_local_record(&tx, test_record('C'))?;

        ri.change_local_guid(
            &tx,
            &SyncGuid::new(&expand_test_guid('C')),
            &SyncGuid::new(&expand_test_guid('B')),
        )?;
        tx.commit()?;
        assert!(get_credit_card(&db.writer, &expand_test_guid('C').into()).is_err());
        assert!(get_credit_card(&db.writer, &expand_test_guid('B').into()).is_ok());
        Ok(())
    }

    #[test]
    fn test_get_incoming() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ci = IncomingCreditCardsImpl {};
        let record = test_record('C');
        let payload = record.clone().into_payload().expect("must get a payload");
        do_test_incoming_same(&ci, &tx, record, payload);
    }

    #[test]
    fn test_incoming_tombstone() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ci = IncomingCreditCardsImpl {};
        do_test_incoming_tombstone(&ci, &tx, test_record('C'));
    }

    #[test]
    fn test_staged_to_mirror() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ci = IncomingCreditCardsImpl {};
        let record = test_record('C');
        let payload = record.clone().into_payload().expect("must get a payload");
        do_test_staged_to_mirror(&ci, &tx, record, payload, "credit_cards_mirror");
    }
}

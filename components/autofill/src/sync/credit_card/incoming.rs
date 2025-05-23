/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::CreditCardPayload;
use crate::db::credit_cards::{add_internal_credit_card, update_internal_credit_card};
use crate::db::models::credit_card::InternalCreditCard;
use crate::db::schema::CREDIT_CARD_COMMON_COLS;
use crate::encryption::EncryptorDecryptor;
use crate::error::*;
use crate::sync::common::*;
use crate::sync::{
    IncomingBso, IncomingContent, IncomingEnvelope, IncomingKind, IncomingState, LocalRecordInfo,
    ProcessIncomingRecordImpl, ServerTimestamp, SyncRecord,
};
use interrupt_support::Interruptee;
use rusqlite::{named_params, Transaction};
use sql_support::ConnExt;
use sync_guid::Guid as SyncGuid;

// Takes a raw payload, as stored in our database, and returns an InternalCreditCard
// or a tombstone. Credit-cards store the payload as an encrypted string, so we
// decrypt before conversion.
fn raw_payload_to_incoming(
    id: SyncGuid,
    raw: String,
    encdec: &EncryptorDecryptor,
) -> Result<IncomingContent<InternalCreditCard>> {
    let payload = encdec.decrypt(&raw)?;
    // Turn it into a BSO
    let bso = IncomingBso {
        envelope: IncomingEnvelope {
            id,
            modified: ServerTimestamp::default(),
            sortindex: None,
            ttl: None,
        },
        payload,
    };
    // For hysterical raisins, we use an IncomingContent<CCPayload> to convert
    // to an IncomingContent<InternalCC>
    let payload_content = bso.into_content::<CreditCardPayload>();
    Ok(match payload_content.kind {
        IncomingKind::Content(content) => IncomingContent {
            envelope: payload_content.envelope,
            kind: IncomingKind::Content(InternalCreditCard::from_payload(content, encdec)?),
        },
        IncomingKind::Tombstone => IncomingContent {
            envelope: payload_content.envelope,
            kind: IncomingKind::Tombstone,
        },
        IncomingKind::Malformed => IncomingContent {
            envelope: payload_content.envelope,
            kind: IncomingKind::Malformed,
        },
    })
}

pub(super) struct IncomingCreditCardsImpl {
    pub(super) encdec: EncryptorDecryptor,
}

impl ProcessIncomingRecordImpl for IncomingCreditCardsImpl {
    type Record = InternalCreditCard;

    /// The first step in the "apply incoming" process - stage the records
    fn stage_incoming(
        &self,
        tx: &Transaction<'_>,
        incoming: Vec<IncomingBso>,
        signal: &dyn Interruptee,
    ) -> Result<()> {
        // Convert the sync15::Payloads to encrypted strings.
        let to_stage = incoming
            .into_iter()
            .map(|bso| {
                // consider turning this into malformed?
                let encrypted = self.encdec.encrypt(&bso.payload)?;
                Ok((bso.envelope.id, encrypted, bso.envelope.modified))
            })
            .collect::<Result<_>>()?;
        common_stage_incoming_records(tx, "credit_cards_sync_staging", to_stage, signal)
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
            l.cc_number_enc,
            l.cc_number_last_4,
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

        tx.query_rows_and_then(sql, [], |row| -> Result<IncomingState<Self::Record>> {
            // the 'guid' and 's_payload' rows must be non-null.
            let guid: SyncGuid = row.get("guid")?;
            let incoming =
                raw_payload_to_incoming(guid.clone(), row.get("s_payload")?, &self.encdec)?;
            Ok(IncomingState {
                incoming,
                local: match row.get_unwrap::<_, Option<String>>("l_guid") {
                    Some(l_guid) => {
                        assert_eq!(l_guid, guid);
                        // local record exists, check the state.
                        let record = InternalCreditCard::from_row(row)?;
                        if record.has_scrubbed_data() {
                            LocalRecordInfo::Scrubbed { record }
                        } else {
                            let has_changes = record.metadata().sync_change_counter != 0;
                            if has_changes {
                                LocalRecordInfo::Modified { record }
                            } else {
                                LocalRecordInfo::Unmodified { record }
                            }
                        }
                    }
                    None => {
                        // no local record - maybe a tombstone?
                        match row.get::<_, Option<String>>("t_guid")? {
                            Some(t_guid) => {
                                assert_eq!(guid, t_guid);
                                LocalRecordInfo::Tombstone { guid: guid.clone() }
                            }
                            None => LocalRecordInfo::Missing,
                        }
                    }
                },
                mirror: {
                    match row.get::<_, Option<String>>("m_payload")? {
                        Some(m_payload) => {
                            // a tombstone in the mirror can be treated as though it's missing.
                            raw_payload_to_incoming(guid, m_payload, &self.encdec)?.content()
                        }
                        None => None,
                    }
                },
            })
        })
    }

    /// Returns a local record that has the same values as the given incoming record (with the exception
    /// of the `guid` values which should differ) that will be used as a local duplicate record for
    /// syncing.
    fn get_local_dupe(
        &self,
        tx: &Transaction<'_>,
        incoming: &Self::Record,
    ) -> Result<Option<Self::Record>> {
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
                -- and sql can check the field values (but note we can not meaningfully
                -- check the encrypted value, as it's different each time it is encrypted)
                AND cc_name == :cc_name
                AND cc_number_last_4 == :cc_number_last_4
                AND cc_exp_month == :cc_exp_month
                AND cc_exp_year == :cc_exp_year
                AND cc_type == :cc_type", common_cols = CREDIT_CARD_COMMON_COLS);

        let params = named_params! {
            ":guid": incoming.guid,
            ":cc_name": incoming.cc_name,
            ":cc_number_last_4": incoming.cc_number_last_4,
            ":cc_exp_month": incoming.cc_exp_month,
            ":cc_exp_year": incoming.cc_exp_year,
            ":cc_type": incoming.cc_type,
        };

        // Because we can't check the number in the sql, we fetch all matching
        // rows and decrypt the numbers here.
        let records = tx.query_rows_and_then(&sql, params, |row| -> Result<Self::Record> {
            Ok(Self::Record::from_row(row)?)
        })?;

        let incoming_cc_number = self.encdec.decrypt(&incoming.cc_number_enc)?;
        for record in records {
            if self.encdec.decrypt(&record.cc_number_enc)? == incoming_cc_number {
                return Ok(Some(record));
            }
        }
        Ok(None)
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
    /// We also update the mirror record if it exists in forking scenarios
    fn change_record_guid(
        &self,
        tx: &Transaction<'_>,
        old_guid: &SyncGuid,
        new_guid: &SyncGuid,
    ) -> Result<()> {
        common_change_guid(
            tx,
            "credit_cards_data",
            "credit_cards_mirror",
            old_guid,
            new_guid,
        )
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

    use error_support::{info, trace};
    use interrupt_support::NeverInterrupts;
    use nss::ensure_initialized;
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
                        "cc-number": "1234567812345678",
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
                        "cc-number": "8765432112345678",
                        "cc-exp-month": 1,
                        "cc-exp-year": 2020,
                        "cc-type": "visa",
                        "timeCreated": 0,
                        "timeLastUsed": 0,
                        "timeLastModified": 0,
                        "timesUsed": 0,
                        "version": 3,
                    }
                },
                "D" : {
                    "id": expand_test_guid('D'),
                    "entry": {
                        "cc-name": "Mr Me Another Person",
                        "cc-number": "8765432112345678",
                        "cc-exp-month": 1,
                        "cc-exp-year": 2020,
                        "cc-type": "visa",
                        "timeCreated": 0,
                        "timeLastUsed": 0,
                        "timeLastModified": 0,
                        "timesUsed": 0,
                        "version": 3,
                        "foo": "bar",
                        "baz": "qux",
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

    fn test_record(guid_prefix: char, encdec: &EncryptorDecryptor) -> InternalCreditCard {
        let json = test_json_record(guid_prefix);
        let payload = serde_json::from_value(json).unwrap();
        InternalCreditCard::from_payload(payload, encdec).expect("should be valid")
    }

    #[test]
    fn test_stage_incoming() -> Result<()> {
        ensure_initialized();
        error_support::init_for_tests();
        let mut db = new_syncable_mem_db();
        struct TestCase {
            incoming_records: Vec<Value>,
            mirror_records: Vec<Value>,
            expected_record_count: usize,
            expected_tombstone_count: usize,
        }

        let test_cases = vec![
            TestCase {
                incoming_records: vec![test_json_record('A')],
                mirror_records: vec![],
                expected_record_count: 1,
                expected_tombstone_count: 0,
            },
            TestCase {
                incoming_records: vec![test_json_tombstone('A')],
                mirror_records: vec![],
                expected_record_count: 0,
                expected_tombstone_count: 1,
            },
            TestCase {
                incoming_records: vec![
                    test_json_record('A'),
                    test_json_record('C'),
                    test_json_tombstone('B'),
                ],
                mirror_records: vec![],
                expected_record_count: 2,
                expected_tombstone_count: 1,
            },
            // incoming tombstone with existing tombstone in the mirror
            TestCase {
                incoming_records: vec![test_json_tombstone('B')],
                mirror_records: vec![test_json_tombstone('B')],
                expected_record_count: 0,
                expected_tombstone_count: 1,
            },
        ];

        for tc in test_cases {
            info!("starting new testcase");
            let tx = db.transaction().unwrap();
            let encdec = EncryptorDecryptor::new_with_random_key().unwrap();

            // Add required items to the mirrors.
            let mirror_sql = "INSERT OR REPLACE INTO credit_cards_mirror (guid, payload)
                              VALUES (:guid, :payload)";
            for payload in tc.mirror_records {
                tx.execute(
                    mirror_sql,
                    rusqlite::named_params! {
                        ":guid": payload["id"].as_str().unwrap(),
                        ":payload": encdec.encrypt(&payload.to_string())?,
                    },
                )
                .expect("should insert mirror record");
            }

            let ri = IncomingCreditCardsImpl { encdec };
            ri.stage_incoming(
                &tx,
                array_to_incoming(tc.incoming_records),
                &NeverInterrupts,
            )?;

            let records = tx.conn().query_rows_and_then(
                "SELECT * FROM temp.credit_cards_sync_staging;",
                [],
                |row| -> Result<IncomingContent<InternalCreditCard>> {
                    let guid: SyncGuid = row.get_unwrap("guid");
                    let enc_payload: String = row.get_unwrap("payload");
                    raw_payload_to_incoming(guid, enc_payload, &ri.encdec)
                },
            )?;

            let record_count = records
                .iter()
                .filter(|p| !matches!(p.kind, IncomingKind::Tombstone))
                .count();
            let tombstone_count = records.len() - record_count;
            trace!("record count: {record_count}, tombstone count: {tombstone_count}");

            assert_eq!(record_count, tc.expected_record_count);
            assert_eq!(tombstone_count, tc.expected_tombstone_count);

            ri.fetch_incoming_states(&tx)?;

            tx.execute("DELETE FROM temp.credit_cards_sync_staging;", [])?;
        }
        Ok(())
    }

    #[test]
    fn test_change_record_guid() -> Result<()> {
        ensure_initialized();
        let mut db = new_syncable_mem_db();
        let tx = db.transaction()?;
        let ri = IncomingCreditCardsImpl {
            encdec: EncryptorDecryptor::new_with_random_key().unwrap(),
        };

        ri.insert_local_record(&tx, test_record('C', &ri.encdec))?;

        ri.change_record_guid(
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
        ensure_initialized();
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ci = IncomingCreditCardsImpl {
            encdec: EncryptorDecryptor::new_with_random_key().unwrap(),
        };
        let record = test_record('C', &ci.encdec);
        let bso = record
            .clone()
            .into_test_incoming_bso(&ci.encdec, Default::default());
        do_test_incoming_same(&ci, &tx, record, bso);
    }

    #[test]
    fn test_incoming_tombstone() {
        ensure_initialized();
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ci = IncomingCreditCardsImpl {
            encdec: EncryptorDecryptor::new_with_random_key().unwrap(),
        };
        do_test_incoming_tombstone(&ci, &tx, test_record('C', &ci.encdec));
    }

    #[test]
    fn test_local_data_scrubbed() {
        ensure_initialized();
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ci = IncomingCreditCardsImpl {
            encdec: EncryptorDecryptor::new_with_random_key().unwrap(),
        };
        let mut scrubbed_record = test_record('A', &ci.encdec);
        let bso = scrubbed_record
            .clone()
            .into_test_incoming_bso(&ci.encdec, Default::default());
        scrubbed_record.cc_number_enc = "".to_string();
        do_test_scrubbed_local_data(&ci, &tx, scrubbed_record, bso);
    }

    #[test]
    fn test_staged_to_mirror() {
        ensure_initialized();
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ci = IncomingCreditCardsImpl {
            encdec: EncryptorDecryptor::new_with_random_key().unwrap(),
        };
        let record = test_record('C', &ci.encdec);
        let bso = record
            .clone()
            .into_test_incoming_bso(&ci.encdec, Default::default());
        do_test_staged_to_mirror(&ci, &tx, record, bso, "credit_cards_mirror");
    }

    #[test]
    fn test_find_dupe() {
        ensure_initialized();
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let encdec = EncryptorDecryptor::new_with_random_key().unwrap();
        let ci = IncomingCreditCardsImpl { encdec };
        let local_record = test_record('C', &ci.encdec);
        let local_guid = local_record.guid.clone();
        ci.insert_local_record(&tx, local_record.clone()).unwrap();

        // Now the same record incoming - it should find the one we just added
        // above as a dupe.
        let mut incoming_record = test_record('C', &ci.encdec);
        // sanity check that the encrypted numbers are different even though
        // the decrypted numbers are identical.
        assert_ne!(local_record.cc_number_enc, incoming_record.cc_number_enc);
        // but the other fields the sql checks are
        assert_eq!(local_record.cc_name, incoming_record.cc_name);
        assert_eq!(
            local_record.cc_number_last_4,
            incoming_record.cc_number_last_4
        );
        assert_eq!(local_record.cc_exp_month, incoming_record.cc_exp_month);
        assert_eq!(local_record.cc_exp_year, incoming_record.cc_exp_year);
        assert_eq!(local_record.cc_type, incoming_record.cc_type);
        // change the incoming guid so we don't immediately think they are the same.
        incoming_record.guid = SyncGuid::random();

        // expect `Ok(Some(record))`
        let dupe = ci.get_local_dupe(&tx, &incoming_record).unwrap().unwrap();
        assert_eq!(dupe.guid, local_guid);
    }

    // largely the same test as above, but going through the entire plan + apply
    // cycle.
    #[test]
    fn test_find_dupe_applied() {
        ensure_initialized();
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let encdec = EncryptorDecryptor::new_with_random_key().unwrap();
        let ci = IncomingCreditCardsImpl { encdec };
        let local_record = test_record('C', &ci.encdec);
        let local_guid = local_record.guid.clone();
        ci.insert_local_record(&tx, local_record.clone()).unwrap();

        // Now the same record incoming, but with a different guid. It should
        // find the local one we just added above as a dupe.
        let incoming_guid = SyncGuid::new(&expand_test_guid('I'));
        let mut incoming = local_record;
        incoming.guid = incoming_guid.clone();

        let incoming_state = IncomingState {
            incoming: IncomingContent {
                envelope: IncomingEnvelope {
                    id: incoming_guid.clone(),
                    modified: ServerTimestamp::default(),
                    sortindex: None,
                    ttl: None,
                },
                kind: IncomingKind::Content(incoming),
            },
            // LocalRecordInfo::Missing because we don't have a local record with
            // the incoming GUID.
            local: LocalRecordInfo::Missing,
            mirror: None,
        };

        let incoming_action =
            crate::sync::plan_incoming(&ci, &tx, incoming_state).expect("should get action");
        // We should have found the local as a dupe.
        assert!(
            matches!(incoming_action, crate::sync::IncomingAction::UpdateLocalGuid { ref old_guid, record: ref incoming } if *old_guid == local_guid && incoming.guid == incoming_guid)
        );

        // and apply it.
        crate::sync::apply_incoming_action(&ci, &tx, incoming_action).expect("should apply");

        // and the local record should now have the incoming guid.
        tx.commit().expect("should commit");
        assert!(get_credit_card(&db.writer, &local_guid).is_err());
        assert!(get_credit_card(&db.writer, &incoming_guid).is_ok());
    }

    #[test]
    fn test_get_incoming_unknown_fields() {
        ensure_initialized();
        let json = test_json_record('D');
        let cc_payload = serde_json::from_value::<CreditCardPayload>(json).unwrap();
        // The incoming payload should've correctly deserialized any unknown_fields into a Map<String,Value>
        assert_eq!(cc_payload.entry.unknown_fields.len(), 2);
        assert_eq!(
            cc_payload
                .entry
                .unknown_fields
                .get("foo")
                .unwrap()
                .as_str()
                .unwrap(),
            "bar"
        );
    }
}

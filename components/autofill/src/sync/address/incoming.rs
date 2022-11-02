/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::addresses::{add_internal_address, update_internal_address};
use crate::db::models::address::InternalAddress;
use crate::db::schema::ADDRESS_COMMON_COLS;
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

// Takes a raw payload, as stored in our database, and returns an InternalAddress
// or a tombstone. Addresses store the raw payload as cleartext json.
fn raw_payload_to_incoming(raw: String) -> Result<IncomingRecord<InternalAddress>> {
    let payload = serde_json::from_str::<Payload>(&raw)?;
    Ok(if payload.is_tombstone() {
        IncomingRecord::Tombstone {
            guid: payload.id().into(),
        }
    } else {
        IncomingRecord::Record {
            record: InternalAddress::from_payload(payload.into_record()?)?,
        }
    })
}

pub(super) struct IncomingAddressesImpl {}

impl ProcessIncomingRecordImpl for IncomingAddressesImpl {
    type Record = InternalAddress;

    /// The first step in the "apply incoming" process - stage the records
    fn stage_incoming(
        &self,
        tx: &Transaction<'_>,
        incoming: Vec<(Payload, ServerTimestamp)>,
        signal: &dyn Interruptee,
    ) -> Result<()> {
        let to_stage = incoming
            .into_iter()
            .map(|(payload, timestamp)| {
                let guid = SyncGuid::new(payload.id());
                let cleartext = payload.into_json_string();
                (guid, cleartext, timestamp)
            })
            .collect();
        common_stage_incoming_records(tx, "addresses_sync_staging", to_stage, signal)
    }

    fn finish_incoming(&self, tx: &Transaction<'_>) -> Result<()> {
        common_mirror_staged_records(tx, "addresses_sync_staging", "addresses_mirror")
    }

    /// The second step in the "apply incoming" process for syncing autofill address records.
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
            l.given_name,
            l.additional_name,
            l.family_name,
            l.organization,
            l.street_address,
            l.address_level3,
            l.address_level2,
            l.address_level1,
            l.postal_code,
            l.country,
            l.tel,
            l.email,
            l.time_created,
            l.time_last_used,
            l.time_last_modified,
            l.times_used,
            l.sync_change_counter
        FROM temp.addresses_sync_staging s
        LEFT JOIN addresses_mirror m ON s.guid = m.guid
        LEFT JOIN addresses_data l ON s.guid = l.guid
        LEFT JOIN addresses_tombstones t ON s.guid = t.guid";

        tx.query_rows_and_then(sql, [], |row| -> Result<IncomingState<Self::Record>> {
            // the 'guid' and 's_payload' rows must be non-null.
            let guid: SyncGuid = row.get("guid")?;
            // turn it into a sync15::Payload
            let incoming = raw_payload_to_incoming(row.get("s_payload")?)?;
            Ok(IncomingState {
                incoming,
                local: match row.get_unwrap::<_, Option<String>>("l_guid") {
                    Some(l_guid) => {
                        assert_eq!(l_guid, guid);
                        // local record exists, check the state.
                        let record = InternalAddress::from_row(row)?;
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
                            // a tombstone in the mirror can be treated as though it's missing.
                            match raw_payload_to_incoming(m_payload)? {
                                IncomingRecord::Record { record } => Some(record),
                                IncomingRecord::Tombstone { .. } => None,
                            }
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
            FROM addresses_data
            WHERE
                -- `guid <> :guid` is a pre-condition for this being called, but...
                guid <> :guid
                -- only non-synced records are candidates, which means can't already be in the mirror.
                AND guid NOT IN (
                    SELECT guid
                    FROM addresses_mirror
                )
                -- and sql can check the field values.
                AND given_name == :given_name
                AND additional_name == :additional_name
                AND family_name == :family_name
                AND organization == :organization
                AND street_address == :street_address
                AND address_level3 == :address_level3
                AND address_level2 == :address_level2
                AND address_level1 == :address_level1
                AND postal_code == :postal_code
                AND country == :country
                AND tel == :tel
                AND email == :email", common_cols = ADDRESS_COMMON_COLS);

        let params = named_params! {
            ":guid": incoming.guid,
            ":given_name": incoming.given_name,
            ":additional_name": incoming.additional_name,
            ":family_name": incoming.family_name,
            ":organization": incoming.organization,
            ":street_address": incoming.street_address,
            ":address_level3": incoming.address_level3,
            ":address_level2": incoming.address_level2,
            ":address_level1": incoming.address_level1,
            ":postal_code": incoming.postal_code,
            ":country": incoming.country,
            ":tel": incoming.tel,
            ":email": incoming.email,
        };

        let result = tx.query_row(&sql, params, |row| {
            Ok(Self::Record::from_row(row).expect("wtf? '?' doesn't work :("))
        });

        match result {
            Ok(r) => Ok(Some(r)),
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
        update_internal_address(tx, &new_record, flag_as_changed)?;
        Ok(())
    }

    fn insert_local_record(&self, tx: &Transaction<'_>, new_record: Self::Record) -> Result<()> {
        add_internal_address(tx, &new_record)?;
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
        common_change_guid(tx, "addresses_data", old_guid, new_guid)
    }

    fn remove_record(&self, tx: &Transaction<'_>, guid: &SyncGuid) -> Result<()> {
        common_remove_record(tx, "addresses_data", guid)
    }

    fn remove_tombstone(&self, tx: &Transaction<'_>, guid: &SyncGuid) -> Result<()> {
        common_remove_record(tx, "addresses_tombstones", guid)
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::test::new_syncable_mem_db;
    use super::*;
    use crate::db::addresses::get_address;
    use crate::sync::common::tests::*;

    use interrupt_support::NeverInterrupts;
    use serde_json::{json, Map, Value};
    use sql_support::ConnExt;

    impl InternalAddress {
        fn into_sync_payload(self) -> sync15::Payload {
            sync15::Payload::from_record(self.into_payload().expect("is json"))
                .expect("can get sync payload")
        }
    }

    lazy_static::lazy_static! {
        static ref TEST_JSON_RECORDS: Map<String, Value> = {
            // NOTE: the JSON here is the same as stored on the sync server -
            // the superfluous `entry` is unfortunate but from desktop.
            let val = json! {{
                "A" : {
                    "id": expand_test_guid('A'),
                    "entry": {
                        "givenName": "john",
                        "familyName": "doe",
                        "streetAddress": "1300 Broadway",
                        "addressLevel2": "New York, NY",
                        "country": "United States",
                        "version": 1,
                    }
                },
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
        let address_payload = serde_json::from_value(json).unwrap();
        InternalAddress::from_payload(address_payload).expect("should be valid")
    }

    #[test]
    fn test_stage_incoming() -> Result<()> {
        let _ = env_logger::try_init();
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
            log::info!("starting new testcase");
            let tx = db.transaction()?;

            // Add required items to the mirrors.
            let mirror_sql = "INSERT OR REPLACE INTO addresses_mirror (guid, payload)
                              VALUES (:guid, :payload)";
            for payload in tc.mirror_records {
                tx.execute(
                    mirror_sql,
                    rusqlite::named_params! {
                        ":guid": payload["id"].as_str().unwrap(),
                        ":payload": payload.to_string(),
                    },
                )
                .expect("should insert mirror record");
            }

            let ri = IncomingAddressesImpl {};
            ri.stage_incoming(
                &tx,
                array_to_incoming(tc.incoming_records),
                &NeverInterrupts,
            )?;

            let payloads = tx.conn().query_rows_and_then(
                "SELECT * FROM temp.addresses_sync_staging;",
                [],
                |row| -> Result<Payload> {
                    let payload: String = row.get_unwrap("payload");
                    Ok(Payload::from_json(serde_json::from_str(&payload)?)?)
                },
            )?;

            let record_count = payloads.iter().filter(|p| !p.is_tombstone()).count();
            let tombstone_count = payloads.len() - record_count;

            assert_eq!(record_count, tc.expected_record_count);
            assert_eq!(tombstone_count, tc.expected_tombstone_count);

            ri.fetch_incoming_states(&tx)?;

            tx.execute("DELETE FROM temp.addresses_sync_staging;", [])?;
        }
        Ok(())
    }

    #[test]
    fn test_change_local_guid() -> Result<()> {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction()?;
        let ri = IncomingAddressesImpl {};

        ri.insert_local_record(&tx, test_record('C'))?;

        ri.change_local_guid(
            &tx,
            &SyncGuid::new(&expand_test_guid('C')),
            &SyncGuid::new(&expand_test_guid('B')),
        )?;
        tx.commit()?;
        assert!(get_address(&db.writer, &expand_test_guid('C').into()).is_err());
        assert!(get_address(&db.writer, &expand_test_guid('B').into()).is_ok());
        Ok(())
    }

    #[test]
    fn test_get_incoming() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ai = IncomingAddressesImpl {};
        let record = test_record('C');
        let payload = record.clone().into_sync_payload();
        do_test_incoming_same(&ai, &tx, record, payload);
    }

    #[test]
    fn test_incoming_tombstone() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ai = IncomingAddressesImpl {};
        do_test_incoming_tombstone(&ai, &tx, test_record('C'));
    }

    #[test]
    fn test_staged_to_mirror() {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction().expect("should get tx");
        let ai = IncomingAddressesImpl {};
        let record = test_record('C');
        let payload = record.clone().into_sync_payload();
        do_test_staged_to_mirror(&ai, &tx, record, payload, "addresses_mirror");
    }
}

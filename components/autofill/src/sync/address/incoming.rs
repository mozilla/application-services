/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::AddressPayload;
use crate::db::addresses::{add_internal_address, update_internal_address};
use crate::db::models::address::InternalAddress;
use crate::db::schema::ADDRESS_COMMON_COLS;
use crate::error::*;
use crate::sync::address::name_utils::{join_name_parts, split_name, NameParts};
use crate::sync::common::*;
use crate::sync::{
    IncomingBso, IncomingContent, IncomingEnvelope, IncomingKind, IncomingState, LocalRecordInfo,
    ProcessIncomingRecordImpl, ServerTimestamp, SyncRecord,
};
use interrupt_support::Interruptee;
use rusqlite::{named_params, Transaction};
use sql_support::ConnExt;
use sync_guid::Guid as SyncGuid;

// When an incoming record lacks the `name` field but includes any `*_name` fields, we can
// assume that the record originates from an older device.

// If the record comes from an older device, we compare the `*_name` fields with those in
// the corresponding local record. If the values of the `*_name`
// fields differ, it indicates that the incoming record has updated these fields. If the
// values are the same, we replace the name field of the incoming record with the local
// name field to ensure the completeness of the name field when reconciling.
//
// Here is an example:
// Assume the local record is {"name": "Mr. John Doe"}. If an updated incoming record
// has {"given_name": "John", "family_name": "Doe"}, we will NOT join the `*_name` fields
// and replace the local `name` field with "John Doe". This allows us to retain the complete
// name - "Mr. John Doe".
// However, if the updated incoming record has {"given_name": "Jane", "family_name": "Poe"},
// we will rebuild it and replace the local `name` field with "Jane Poe".
fn update_name(payload_content: &mut IncomingContent<AddressPayload>, local_name: String) {
    // Check if the kind is IncomingKind::Content and get a mutable reference to internal_address
    let internal_address =
        if let IncomingKind::Content(internal_address) = &mut payload_content.kind {
            internal_address
        } else {
            return;
        };

    let entry = &mut internal_address.entry;

    // Return early if the name is not empty or `*-name`` parts are empty
    if !entry.name.is_empty()
        || (entry.given_name.is_empty()
            && entry.additional_name.is_empty()
            && entry.family_name.is_empty())
    {
        return;
    }

    // Split the local name into its parts
    let NameParts {
        given,
        middle,
        family,
    } = split_name(&local_name);

    // Check if the local name matches the entry names
    let is_local_name_matching =
        entry.given_name == given && entry.additional_name == middle && entry.family_name == family;

    // Update the name based on whether the local name matches
    entry.name = if is_local_name_matching {
        local_name
    } else {
        join_name_parts(&NameParts {
            given: entry.given_name.clone(),
            middle: entry.additional_name.clone(),
            family: entry.family_name.clone(),
        })
    };
}

fn create_incoming_bso(id: SyncGuid, raw: String) -> IncomingContent<AddressPayload> {
    let bso = IncomingBso {
        envelope: IncomingEnvelope {
            id,
            modified: ServerTimestamp::default(),
            sortindex: None,
            ttl: None,
        },
        payload: raw,
    };
    bso.into_content::<AddressPayload>()
}

fn bso_to_incoming(
    payload_content: IncomingContent<AddressPayload>,
) -> Result<IncomingContent<InternalAddress>> {
    Ok(match payload_content.kind {
        IncomingKind::Content(content) => IncomingContent {
            envelope: payload_content.envelope,
            kind: IncomingKind::Content(InternalAddress::from_payload(content)?),
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

// Takes a raw payload, as stored in our database, and returns an InternalAddress
// or a tombstone. Addresses store the raw payload as cleartext json.
fn raw_payload_to_incoming(id: SyncGuid, raw: String) -> Result<IncomingContent<InternalAddress>> {
    let payload_content = create_incoming_bso(id, raw);

    Ok(match payload_content.kind {
        IncomingKind::Content(content) => IncomingContent {
            envelope: payload_content.envelope,
            kind: IncomingKind::Content(InternalAddress::from_payload(content)?),
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

pub(super) struct IncomingAddressesImpl {}

impl ProcessIncomingRecordImpl for IncomingAddressesImpl {
    type Record = InternalAddress;

    /// The first step in the "apply incoming" process - stage the records
    fn stage_incoming(
        &self,
        tx: &Transaction<'_>,
        incoming: Vec<IncomingBso>,
        signal: &dyn Interruptee,
    ) -> Result<()> {
        let to_stage = incoming
            .into_iter()
            // We persist the entire payload as cleartext - which it already is!
            .map(|bso| (bso.envelope.id, bso.payload, bso.envelope.modified))
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
            l.name,
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

            // We update the 'name' field using the update_name function.
            // We utilize create_incoming_bso and bso_to_incoming functions
            // instead of payload_to_incoming. This is done to avoid directly passing
            // row.get("name") to payload_to_incoming, which would result in having to pass
            // None parameters in a few places.
            let mut payload_content = create_incoming_bso(guid.clone(), row.get("s_payload")?);
            update_name(
                &mut payload_content,
                row.get("name").unwrap_or("".to_string()),
            );
            let incoming = bso_to_incoming(payload_content)?;

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
                            raw_payload_to_incoming(guid, m_payload)?.content()
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
                AND name == :name
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
            ":name": incoming.name,
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
    /// We also update the mirror record if it exists in forking scenarios
    fn change_record_guid(
        &self,
        tx: &Transaction<'_>,
        old_guid: &SyncGuid,
        new_guid: &SyncGuid,
    ) -> Result<()> {
        common_change_guid(tx, "addresses_data", "addresses_mirror", old_guid, new_guid)
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

    use error_support::info;
    use interrupt_support::NeverInterrupts;
    use serde_json::{json, Map, Value};
    use sql_support::ConnExt;

    impl InternalAddress {
        fn into_test_incoming_bso(self) -> IncomingBso {
            IncomingBso::from_test_content(self.into_payload().expect("is json"))
        }
    }

    lazy_static::lazy_static! {
        static ref TEST_JSON_RECORDS: Map<String, Value> = {
            // NOTE: the JSON here is the same as stored on the sync server -
            // the superfluous `entry` is unfortunate but from desktop.
            // JSON from the server is kebab-style, EXCEPT the times{X} fields
            // see PayloadEntry struct
            let val = json! {{
                "A" : {
                    "id": expand_test_guid('A'),
                    "entry": {
                        "name": "john doe",
                        "given-name": "john",
                        "family-name": "doe",
                        "street-address": "1300 Broadway",
                        "address-level2": "New York, NY",
                        "country": "United States",
                        "version": 1,
                    }
                },
                "C" : {
                    "id": expand_test_guid('C'),
                    "entry": {
                        "name": "jane doe",
                        "given-name": "jane",
                        "family-name": "doe",
                        "street-address": "3050 South La Brea Ave",
                        "address-level2": "Los Angeles, CA",
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
                        "name": "test1 test2",
                        "given-name": "test1",
                        "family-name": "test2",
                        "street-address": "85 Pike St",
                        "address-level2": "Seattle, WA",
                        "country": "United States",
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
        let address_payload = serde_json::from_value(json).unwrap();
        InternalAddress::from_payload(address_payload).expect("should be valid")
    }

    #[test]
    fn test_stage_incoming() -> Result<()> {
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

            let records = tx.conn().query_rows_and_then(
                "SELECT * FROM temp.addresses_sync_staging;",
                [],
                |row| -> Result<IncomingContent<InternalAddress>> {
                    let guid: SyncGuid = row.get_unwrap("guid");
                    let payload: String = row.get_unwrap("payload");
                    raw_payload_to_incoming(guid, payload)
                },
            )?;

            let record_count = records
                .iter()
                .filter(|p| !matches!(p.kind, IncomingKind::Tombstone))
                .count();
            let tombstone_count = records.len() - record_count;

            assert_eq!(record_count, tc.expected_record_count);
            assert_eq!(tombstone_count, tc.expected_tombstone_count);

            ri.fetch_incoming_states(&tx)?;

            tx.execute("DELETE FROM temp.addresses_sync_staging;", [])?;
        }
        Ok(())
    }

    #[test]
    fn test_change_record_guid() -> Result<()> {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction()?;
        let ri = IncomingAddressesImpl {};

        ri.insert_local_record(&tx, test_record('C'))?;

        ri.change_record_guid(
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
        let bso = record.clone().into_test_incoming_bso();
        do_test_incoming_same(&ai, &tx, record, bso);
    }

    #[test]
    fn test_get_incoming_unknown_fields() {
        let json = test_json_record('D');
        let address_payload = serde_json::from_value::<AddressPayload>(json).unwrap();
        // The incoming payload should've correctly deserialized any unknown_fields into a Map<String,Value>
        assert_eq!(address_payload.entry.unknown_fields.len(), 2);
        assert_eq!(
            address_payload
                .entry
                .unknown_fields
                .get("foo")
                .unwrap()
                .as_str()
                .unwrap(),
            "bar"
        );
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
        let bso = record.clone().into_test_incoming_bso();
        do_test_staged_to_mirror(&ai, &tx, record, bso, "addresses_mirror");
    }
}

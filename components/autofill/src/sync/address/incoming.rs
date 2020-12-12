/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::{Record, RecordData};
use crate::error::*;
use interrupt_support::Interruptee;
use rusqlite::{types::ToSql, Connection};
use sync15::Payload;

pub fn stage_incoming(
    conn: &Connection,
    incoming_payloads: Vec<Payload>,
    signal: &dyn Interruptee,
) -> Result<()> {
    let mut incoming_records = Vec::with_capacity(incoming_payloads.len());
    let mut incoming_tombstones = Vec::with_capacity(incoming_payloads.len());

    for payload in incoming_payloads {
        match payload.deleted {
            true => incoming_tombstones.push(payload.into_record::<Record>().unwrap()),
            false => incoming_records.push(payload.into_record::<Record>().unwrap()),
        };
    }
    if incoming_records.len() > 0 {
        save_incoming_records(conn, incoming_records, signal)?;
    }

    if incoming_tombstones.len() > 0 {
        save_incoming_tombstones(conn, incoming_tombstones, signal)?;
    }
    Ok(())
}

fn save_incoming_records(
    conn: &Connection,
    incoming_records: Vec<Record>,
    signal: &dyn Interruptee,
) -> Result<()> {
    let chunk_size = 13;
    sql_support::each_sized_chunk(
        &incoming_records,
        sql_support::default_max_variable_number() / chunk_size,
        |chunk, _| -> Result<()> {
            let sql = format!(
                "INSERT OR REPLACE INTO temp.addresses_staging (
                    guid,
                    given_name,
                    additional_name,
                    family_name,
                    organization,
                    street_address,
                    address_level3,
                    address_level2,
                    address_level1,
                    postal_code,
                    country,
                    tel,
                    email
                ) VALUES {}",
                sql_support::repeat_multi_values(chunk.len(), chunk_size)
            );
            let mut params = Vec::with_capacity(chunk.len() * chunk_size);
            for record in chunk {
                signal.err_if_interrupted()?;
                params.push(&record.guid as &dyn ToSql);
                params.push(&record.data.given_name);
                params.push(&record.data.additional_name);
                params.push(&record.data.family_name);
                params.push(&record.data.organization);
                params.push(&record.data.street_address);
                params.push(&record.data.address_level3);
                params.push(&record.data.address_level2);
                params.push(&record.data.address_level1);
                params.push(&record.data.postal_code);
                params.push(&record.data.country);
                params.push(&record.data.tel);
                params.push(&record.data.email);
            }
            conn.execute(&sql, &params)?;
            Ok(())
        },
    )
}

fn save_incoming_tombstones(
    conn: &Connection,
    incoming_records: Vec<Record>,
    signal: &dyn Interruptee,
) -> Result<()> {
    let chunk_size = 1;
    sql_support::each_sized_chunk(
        &incoming_records,
        sql_support::default_max_variable_number() / chunk_size,
        |chunk, _| -> Result<()> {
            let sql = format!(
                "INSERT OR REPLACE INTO temp.addresses_tombstone_staging (
                    guid
                ) VALUES {}",
                sql_support::repeat_multi_values(chunk.len(), chunk_size)
            );
            let mut params = Vec::with_capacity(chunk.len() * chunk_size);
            for record in chunk {
                signal.err_if_interrupted()?;
                params.push(&record.guid as &dyn ToSql);
            }
            conn.execute(&sql, &params)?;
            Ok(())
        },
    )
}

#[derive(Debug, PartialEq)]
pub enum IncomingState {
    IncomingOnly {
        guid: String,
        incoming: RecordData,
    },
    IncomingTombstone {
        guid: String,
    },
    HasLocal {
        guid: String,
        incoming: RecordData,
        local: RecordData,
        mirror: Option<RecordData>,
    },
    // In desktop, if a local record with the remote's guid doesn't exist, an attempt is made
    // to find a local dupe of the remote record
    // (https://searchfox.org/mozilla-central/source/browser/extensions/formautofill/FormAutofillSync.jsm#132).
    // `HasLocalDupe` is the state which represents when said dupe is found. This is logic
    // that may need to be updated in the future, but currently exists solely for the purpose of reaching
    // parity with desktop.
    HasLocalDupe {
        guid: String,
        dupe_guid: String,
        dupe: RecordData,
        incoming: RecordData,
        mirror: Option<RecordData>,
    },
    LocalTombstone {
        guid: String,
    },
}

#[derive(Debug, PartialEq)]
pub enum IncomingAction {
    // TODO: Add struct data for enum types
    DeleteLocally,
    MergeLocal, // field by field merge between local, mirror, and remote
    MergeDupe,  // field by field merge between local dupe, mirror, and remote
    TakeRemote,
    Nothing,
}

pub fn plan_incoming(s: IncomingState) -> IncomingAction {
    match s {
        IncomingState::IncomingOnly { guid, incoming } => IncomingAction::TakeRemote,
        IncomingState::IncomingTombstone { guid } => IncomingAction::DeleteLocally,
        IncomingState::HasLocal {
            guid,
            incoming,
            local,
            mirror,
        } => IncomingAction::MergeLocal,
        IncomingState::HasLocalDupe {
            guid,
            dupe_guid,
            dupe,
            incoming,
            mirror,
        } => IncomingAction::MergeDupe,
        // It might be better for the `LocalTombstone` state to perform the `DeleteLocally` action.
        // But I think since the store's delete action remove the record from the local table
        // and adds it to the store in a transaction an inconsistent state would be virtually
        // impossible. Also this might require updating the sync counter *if* we add that column
        // to the tombstones table.
        IncomingState::LocalTombstone { guid } => IncomingAction::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::test::new_syncable_mem_db;
    use super::*;

    use interrupt_support::NeverInterrupts;
    use serde_json::{json, Value};
    use sql_support::ConnExt;

    fn array_to_incoming(mut array: Value) -> Vec<Payload> {
        let jv = array.as_array_mut().expect("you must pass a json array");
        let mut result = Vec::with_capacity(jv.len());
        for elt in jv {
            result.push(Payload::from_json(elt.take()).expect("must be valid"));
        }
        result
    }

    #[test]
    fn test_stage_incoming() -> Result<()> {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction()?;
        struct TestCase {
            incoming_records: Value,
            expected_record_count: u32,
            expected_tombstone_count: u32,
        }

        let test_cases = vec![
            TestCase {
                incoming_records: json! {[
                    {
                        "id": "AAAAAAAAAAAAAAAAA",
                        "deleted": false,
                        "givenName": "john",
                        "additionalName": "",
                        "familyName": "doe",
                        "organization": "",
                        "streetAddress": "1300 Broadway",
                        "addressLevel3": "",
                        "addressLevel2": "New York, NY",
                        "addressLevel1": "",
                        "postalCode": "",
                        "country": "United States",
                        "tel": "",
                        "email": "",
                    }
                ]},
                expected_record_count: 1,
                expected_tombstone_count: 0,
            },
            TestCase {
                incoming_records: json! {[
                    {
                        "id": "AAAAAAAAAAAAAA",
                        "deleted": true,
                        "givenName": "",
                        "additionalName": "",
                        "familyName": "",
                        "organization": "",
                        "streetAddress": "",
                        "addressLevel3": "",
                        "addressLevel2": "",
                        "addressLevel1": "",
                        "postalCode": "",
                        "country": "",
                        "tel": "",
                        "email": "",
                    }
                ]},
                expected_record_count: 0,
                expected_tombstone_count: 1,
            },
            TestCase {
                incoming_records: json! {[
                    {
                        "id": "AAAAAAAAAAAAAAAAA",
                        "deleted": false,
                        "givenName": "john",
                        "additionalName": "",
                        "familyName": "doe",
                        "organization": "",
                        "streetAddress": "1300 Broadway",
                        "addressLevel3": "",
                        "addressLevel2": "New York, NY",
                        "addressLevel1": "",
                        "postalCode": "",
                        "country": "United States",
                        "tel": "",
                        "email": "",
                    },
                    {
                        "id": "CCCCCCCCCCCCCCCCCC",
                        "deleted": false,
                        "givenName": "jane",
                        "additionalName": "",
                        "familyName": "doe",
                        "organization": "",
                        "streetAddress": "3050 South La Brea Ave",
                        "addressLevel3": "",
                        "addressLevel2": "Los Angeles, CA",
                        "addressLevel1": "",
                        "postalCode": "",
                        "country": "United States",
                        "tel": "",
                        "email": "",
                    },
                    {
                        "id": "BBBBBBBBBBBBBBBBB",
                        "deleted": true,
                        "givenName": "",
                        "additionalName": "",
                        "familyName": "",
                        "organization": "",
                        "streetAddress": "",
                        "addressLevel3": "",
                        "addressLevel2": "",
                        "addressLevel1": "",
                        "postalCode": "",
                        "country": "",
                        "tel": "",
                        "email": "",
                    }
                ]},
                expected_record_count: 2,
                expected_tombstone_count: 1,
            },
        ];

        for tc in test_cases {
            stage_incoming(
                &tx,
                array_to_incoming(tc.incoming_records),
                &NeverInterrupts,
            )?;

            let record_count: u32 = tx
                .try_query_one("SELECT COUNT(*) FROM temp.addresses_staging", &[], false)
                .expect("get incoming record count")
                .unwrap_or_default();

            let tombstone_count: u32 = tx
                .try_query_one(
                    "SELECT COUNT(*) FROM temp.addresses_tombstone_staging",
                    &[],
                    false,
                )
                .expect("get incoming tombstone count")
                .unwrap_or_default();

            assert_eq!(record_count, tc.expected_record_count);
            assert_eq!(tombstone_count, tc.expected_tombstone_count);

            tx.execute_all(&[
                "DELETE FROM temp.addresses_tombstone_staging;",
                "DELETE FROM temp.addresses_staging;",
            ])?;
        }
        Ok(())
    }
}

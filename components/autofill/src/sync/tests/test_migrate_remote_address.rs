/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::addresses;
use crate::db::schema::create_empty_sync_temp_tables;
use crate::error::Result;
use crate::sync::address::create_engine as create_address_engine;
use crate::sync::{IncomingBso, Metadata};
use crate::{InternalAddress, Store};
use sync15::engine::SyncEngine;
use types::Timestamp;

use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::sync::Arc;
use sync15::{telemetry, ServerTimestamp};
use sync_guid::Guid as SyncGuid;

lazy_static::lazy_static! {
    // NOTE: a guide to reading these test-cases:
    // "parent": What the local record looked like the last time we wrote the
    //         record to the Sync server (ie, what's in our "mirror")
    // "local":  What the local record looks like now. IOW, the differences between
    //         '"parent":' and 'local' are changes recently made which we wish to sync.
    // "remote": An incoming record we need to apply (ie, a record that was possibly
    //         changed on a remote device)
    //
    // These test cases cover the following reconciliation scenarios for the name field:
    // 1. Both local and remote record have no name.
    // 2. Both local and remote record have name field in new format.
    // 3. Local record has name in new format and remote in old.
    // 4. Remote record has name in new format and local in old.
    // 5. Remote and local records have name in different formats, but name parts match.
    // 6. Remote and local records have name in different formats, but name parts are different.
    //
    // To further help understanding this, a few of the testcases are annotated.

    static ref ADDRESS_RECONCILE_TESTCASES: Value = json!([
        // No Local record, reconciled name should be the remote name.
        {
            "description": "Remote is old, no Local",
            "local": [
            ],
            "remote": {
                "version": 1,
                "given-name": "Mark",
                "family-name": "Jones",
                "street-address": "32 Vassar Street",
            },
            "reconciled": {
                "name": "Mark Jones",
                "street-address": "32 Vassar Street",
            },
        },
        // No local record, only remote record with new name format. Reconciled name should be remote name.
        {
            "description": "Remote is new, no Local",
            "local": [
            ],
            "remote": {
                "version": 1,
                "name": "Mr. Mark Jones",
                "street-address": "32 Vassar Street",
            },
            "reconciled": {
                "name": "Mr. Mark Jones",
                "street-address": "32 Vassar Street",
            },
        },
        // No local record name, only remote record with new name format. Reconciled name should be remote name.
        {
            "description": "Remote record doesn't have name, no Local",
            "local": [
            ],
            "remote": {
                "version": 1,
                "street-address": "32 Vassar Street",
            },
            "reconciled": {
                "name": "",
                "street-address": "32 Vassar Street",
            },
        },
        // Both Local and remote records has name in new format. Reconciled name should be remote name.
        {
            "description": "Remote is new, Local is old",
            "parent": {
                "version": 1,
                "name": "Mr. Mark Jones",
                "street-address": "32 Vassar Street",
            },
            "local": [
                {
                    "name": "Mr. Mark Jones",
                    "street-address": "32 Vassar Street",
                },
            ],
            "remote": {
                "version": 1,
                "name": "Mr. John Doe",
                "street-address": "32 Vassar Street",
            },
            "reconciled": {
                "name": "Mr. John Doe",
                "street-address": "32 Vassar Street",
            },
        },
        // No local record, only remote record with new name format. Reconciled name should be remote name.
        {
            "description": "Remote change, new record",
            "parent": {
                "version": 1,
                "name": "Mr. Mark Jones",
                "street-address": "32 Vassar Street",
            },
            "local": [
                {
                    "name": "Mr. Mark Jones",
                    "street-address": "32 Vassar Street",
                },
            ],
            "remote": {
                "version": 1,
                "name": "Mr. John Doe",
                "street-address": "32 Vassar Street",
            },
            "reconciled": {
                "name": "Mr. John Doe",
                "street-address": "32 Vassar Street",
            },
        },
        // Local record and remote name parts match, we use the local name as it has more information.
        {
            "description": "Remote change, old record, name is not updated",
            "parent": {
                "version": 1,
                "name": "Mr. Mark Jones",
                "street-address": "32 Vassar Street",
            },
            "local": [
                {
                    "name": "Mr. Mark Jones",
                    "street-address": "32 Vassar Street",
                },
            ],
            "remote": {
                "version": 1,
                "given-name": "Mark",
                "family-name": "Jones",
                "street-address": "I moved!",
            },
            "reconciled": {
                "name": "Mr. Mark Jones",
                "street-address": "I moved!",
            },
        },
        // Local record and remote name parts don't match, we keep the remote name as is.
        {
            "description": "Remote change, old record, name is updated",
            "parent": {
                "version": 1,
                "name": "Mr. Mark Jones",
                "street-address": "32 Vassar Street",
            },
            "local": [
                {
                    "name": "Mr. Mark Jones",
                    "street-address": "32 Vassar Street",
                },
            ],
            "remote": {
                "version": 1,
                "given-name": "John",
                "family-name": "Doe",
                "street-address": "32 Vassar Street",
            },
            "reconciled": {
                "name": "John Doe",
                "street-address": "32 Vassar Street",
            },
        },
        // Remote record has name but not local, use remote name.
        {
            "description": "Remote change, old record adds name",
            "parent": {
                "version": 1,
                "street-address": "32 Vassar Street",
            },
            "local": [
                {
                    "street-address": "32 Vassar Street",
                },
            ],
            "remote": {
                "version": 1,
                "given-name": "John",
                "family-name": "Doe",
                "street-address": "32 Vassar Street",
            },
            "reconciled": {
                "name": "John Doe",
                "street-address": "32 Vassar Street",
            },
        },
        // No name in either records. Name should be "".
        {
            "description": "Remote change, remote record does not have name",
            "parent": {
                "version": 1,
                "street-address": "32 Vassar Street",
            },
            "local": [
                {
                    "street-address": "32 Vassar Street",
                },
            ],
            "remote": {
                "version": 1,
                "street-address": "I moved!",
            },
            "reconciled": {
                "name": "",
                "street-address": "I moved!",
            },
        },
    ]);
}

// Takes the JSON from one of the tests above and turns it into an IncomingBso,
// suitable for sticking in the mirror or passing to the sync impl.
fn test_to_bso(guid: &SyncGuid, test_payload: &serde_json::Value) -> IncomingBso {
    let json = json!({
        "id": guid.clone(),
        "entry": test_payload.clone(),
    });
    IncomingBso::from_test_content(json)
}

fn check_address_as_expected(address: &InternalAddress, expected: &Map<String, Value>) {
    // InternalAddress doesn't derive Serialize making this a bit painful.
    // 'expected' only has some fields, so we check them individually and explicitly.
    for (name, val) in expected.iter() {
        let name = name.as_ref();
        match name {
            "name" => assert_eq!(val.as_str().unwrap(), address.name),
            "street-address" => assert_eq!(val.as_str().unwrap(), address.street_address),
            "country" => assert_eq!(val.as_str().unwrap(), address.country),
            "tel" => assert_eq!(val.as_str().unwrap(), address.tel),
            "organization" => assert_eq!(val.as_str().unwrap(), address.organization),
            "timeCreated" => assert_eq!(
                Timestamp(val.as_u64().unwrap()),
                address.metadata.time_created
            ),
            "timeLastModified" => assert_eq!(
                Timestamp(val.as_u64().unwrap()),
                address.metadata.time_last_modified
            ),
            "timeLastUsed" => assert_eq!(
                Timestamp(val.as_u64().unwrap()),
                address.metadata.time_last_used
            ),
            "timesUsed" => assert_eq!(val.as_i64().unwrap(), address.metadata.times_used),
            // Sometimes we'll have an `expected_unknown_fields` set for reconciled, we can skip it safely here
            "expected_unknown_fields" => (),
            _ => unreachable!("unexpected field {name}"),
        }
    }
}

// Make a local record, flagged as "changed", from the JSON in our test cases.
fn make_local_from_json(guid: &SyncGuid, json: &serde_json::Value) -> InternalAddress {
    InternalAddress {
        guid: guid.clone(),
        // Note that our test cases only include a subset of possible fields.
        name: json["name"].as_str().unwrap_or_default().to_string(),
        street_address: json["street-address"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        country: json["country"].as_str().unwrap_or_default().to_string(),
        tel: json["tel"].as_str().unwrap_or_default().to_string(),
        organization: json["organization"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        metadata: Metadata {
            time_created: Timestamp(json["timeCreated"].as_u64().unwrap_or_default()),
            time_last_used: Timestamp(json["timeLastUsed"].as_u64().unwrap_or_default()),
            time_last_modified: Timestamp(json["timeLastModified"].as_u64().unwrap_or_default()),
            times_used: json["timesUsed"].as_i64().unwrap_or_default(),
            // all these tests assume local has changed.
            sync_change_counter: 1,
        },
        ..Default::default()
    }
}

// Insert a mirror record from the JSON in our test cases.
fn insert_mirror_record(conn: &Connection, guid: &SyncGuid, test_payload: &serde_json::Value) {
    let bso = test_to_bso(guid, test_payload);
    conn.execute(
        "INSERT OR IGNORE INTO addresses_mirror (guid, payload)
         VALUES (:guid, :payload)",
        rusqlite::named_params! {
            ":guid": bso.envelope.id,
            ":payload": bso.payload,
        },
    )
    .expect("should insert");
}

#[test]
fn test_migrate_remote_addresses() -> Result<()> {
    use error_support::{info, trace};
    error_support::init_for_tests();

    let j = &ADDRESS_RECONCILE_TESTCASES;
    for test_case in j.as_array().unwrap() {
        let desc = test_case["description"].as_str().unwrap();
        let store = Arc::new(Store::new_memory());
        let db = store.db.lock().unwrap();
        let tx = db.unchecked_transaction().unwrap();

        create_empty_sync_temp_tables(&tx)?;
        info!("starting test case: {}", desc);
        // stick the local records in the local DB as real items.
        // Note that some test-cases have multiple "local" records, but that's
        // to explicitly test desktop's version of the "mirror", and doesn't
        // make sense here - we just want the last one.
        let local_array = test_case["local"].as_array().unwrap();
        let guid = if local_array.is_empty() {
            // no local record in this test case, so allocate a random guid.
            trace!("local record: doesn't exist");
            SyncGuid::random()
        } else {
            let local = local_array.last().unwrap();
            trace!("local record: {local}");
            let guid = SyncGuid::random();
            addresses::add_internal_address(&tx, &make_local_from_json(&guid, local))?;

            let mut parent_json = test_case["parent"].clone();
            // we need to add an 'id' entry, the same as the local item we added.
            let map = parent_json.as_object_mut().unwrap();
            map.insert("id".to_string(), serde_json::to_value(guid.clone())?);
            trace!("parent record: {:?}", parent_json);
            insert_mirror_record(&tx, &guid, &parent_json);

            guid
        };

        tx.commit().expect("should commit");

        // convert "incoming" items into payloads and have the sync engine apply them.
        let mut remote = test_case["remote"].clone();
        trace!("remote record: {:?}", remote);
        // we need to add an 'id' entry, the same as the local item we added.
        let map = remote.as_object_mut().unwrap();
        map.insert("id".to_string(), serde_json::to_value(guid.clone())?);

        let bso = test_to_bso(&guid, &remote);
        let remote_time = ServerTimestamp(0);
        let mut telem = telemetry::Engine::new("addresses");

        std::mem::drop(db); // unlock the mutex for the engine.
        let engine = create_address_engine(Arc::clone(&store));

        engine
            .stage_incoming(vec![bso], &mut telem)
            .expect("should stage");

        let outgoing = engine.apply(remote_time, &mut telem).expect("should apply");
        // For some tests, we want to check that the outgoing has what we're expecting
        // to go to the server
        if let Some(outgoing_expected) = test_case.get("outgoing") {
            trace!("Testing outgoing changeset: {:?}", outgoing);
            let bso_payload: Map<String, Value> =
                serde_json::from_str(&outgoing[0].payload).unwrap();
            let entry = bso_payload.get("entry").unwrap();
            let oeb = outgoing_expected.as_object().unwrap();

            // Verify all fields we want tested are in the payload
            for expected in oeb {
                assert_eq!(entry.get(expected.0).unwrap(), expected.1);
            }
        };

        // get a DB reference back to we can check the results.
        let db = store.db.lock().unwrap();

        let all = addresses::get_all_addresses(&db)?;

        // If the JSON has "forked", then we expect 2 different addresses.
        let reconciled = match test_case.get("forked") {
            Some(forked) => {
                let forked = forked.as_object().unwrap();
                assert_eq!(all.len(), 2, "should get a forked address");
                if all[0].guid == guid {
                    check_address_as_expected(&all[1], forked);
                    &all[0]
                } else {
                    assert_eq!(all[1].guid, guid); // lost the local record?
                    check_address_as_expected(&all[0], forked);
                    &all[1]
                }
            }
            None => {
                assert_eq!(all.len(), 1, "should only be one address");
                assert_eq!(all[0].guid, guid);
                &all[0]
            }
        };
        let expected = test_case["reconciled"].as_object().unwrap();
        check_address_as_expected(reconciled, expected);

        // If the reconciled json has `expected_unknown_fields` then we want to validate that the mirror
        // DB has the fields we're trying to roundtrip
        if let Some(unknown_fields) = expected.get("expected_unknown_fields") {
            let tx = db.unchecked_transaction().unwrap();
            let mut stmt = tx.prepare("SELECT payload FROM addresses_mirror")?;
            let rows = stmt.query_map([], |row| row.get(0)).unwrap();

            for row in rows {
                let payload_str: String = row.unwrap();
                let payload: Value = serde_json::from_str(&payload_str).unwrap();
                let entry = payload.get("entry").unwrap();

                // There's probably multiple rows in the mirror, we only want to test against the
                // record we reconciled
                if expected.get("name").unwrap() == entry.get("name").unwrap() {
                    let expected_unknown = unknown_fields.as_object().unwrap();
                    for expected in expected_unknown {
                        assert_eq!(entry.get(expected.0).unwrap(), expected.1);
                    }
                }
            }
        };
    }
    Ok(())
}

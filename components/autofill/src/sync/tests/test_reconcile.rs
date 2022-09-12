/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

// This is a "port" of the desktop xpcshell test named test_reconcile.js.

// NOTE: a guide to reading these test-cases:
// "parent": What the local record looked like the last time we wrote the
//         record to the Sync server (ie, what's in our "mirror")
// "local":  What the local record looks like now. IOW, the differences between
//         '"parent":' and 'local' are changes recently made which we wish to sync.
// "remote": An incoming record we need to apply (ie, a record that was possibly
//         changed on a remote device)
//
// To further help understanding this, a few of the testcases are annotated.

use crate::db::addresses;
use crate::db::schema::create_empty_sync_temp_tables;
use crate::error::Result;
use crate::sync::address::create_engine as create_address_engine;
use crate::sync::Metadata;
use crate::{InternalAddress, Store};
use types::Timestamp;

use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::sync::Arc;
use sync15::telemetry;
use sync15::IncomingChangeset;
use sync15::ServerTimestamp;
use sync15_traits::SyncEngine;
use sync_guid::Guid as SyncGuid;

lazy_static::lazy_static! {
    // NOTE: it would seem nice to stick this JSON in a file which we
    // `include_str!` and parse at runtime - however, we then lose the ability
    // to have comments embedded, and the comments have real value, so...
    static ref ADDRESS_RECONCILE_TESTCASES: Value = json!([
        {
            "description": "Local change",
            "parent": {
                // So when we last wrote the record to the server, it had these values.
                "version": 1,
                "given-name": "Mark",
                "family-name": "Jones",
            },
            "local": [
                {
                    // The current local record - by comparing against parent we can see that
                    // only the given-name has changed locally.
                    "given-name": "Skip",
                    "family-name": "Jones",
                },
            ],
            "remote": {
                // This is the incoming record. It has the same values as parent, so
                // we can deduce the record hasn't actually been changed remotely so we
                // can safely ignore the incoming record and write our local changes.
                "version": 1,
                "given-name": "Mark",
                "family-name": "Jones",
            },
            "reconciled": {
                "given-name": "Skip",
                "family-name": "Jones",
            },
        },
        {
            "description": "Remote change",
            "parent": {
                "version": 1,
                "given-name": "Mark",
                "family-name": "Jones",
            },
            "local": [
                {
                    "given-name": "Mark",
                    "family-name": "Jones",
                },
            ],
            "remote": {
                "version": 1,
                "given-name": "Skip",
                "family-name": "Jones",
            },
            "reconciled": {
                "given-name": "Skip",
                "family-name": "Jones",
            },
        },
    {
        "description": "New local field",
        "parent": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
        },
        "local": [
            {
                "given-name": "Mark",
                "family-name": "Jones",
                "tel": "123456",
            },
        ],
        "remote": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
        },
        "reconciled": {
            "given-name": "Mark",
            "family-name": "Jones",
            "tel": "123456",
        },
    },
    {
        "description": "New remote field",
        "parent": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
        },
        "local": [
            {
                "given-name": "Mark",
                "family-name": "Jones",
            },
        ],
        "remote": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "tel": "123456",
        },
        "reconciled": {
            "given-name": "Mark",
            "family-name": "Jones",
            "tel": "123456",
        },
    },
    {
        "description": "Deleted field locally",
        "parent": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "tel": "123456",
        },
        "local": [
            {
                "given-name": "Mark",
                "family-name": "Jones",
            },
        ],
        "remote": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "tel": "123456",
        },
        "reconciled": {
            "given-name": "Mark",
            "family-name": "Jones",
        },
    },
    {
        "description": "Deleted field remotely",
        "parent": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "tel": "123456",
        },
        "local": [
            {
                "given-name": "Mark",
                "family-name": "Jones",
                "tel": "123456",
            },
        ],
        "remote": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
        },
        "reconciled": {
            "given-name": "Mark",
            "family-name": "Jones",
        },
    },
    {
        "description": "Local and remote changes to unrelated fields",
        "parent": {
            // The last time we wrote this to the server, country was NZ.
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "country": "NZ",
        },
        "local": [
            {
                // The current local record - so locally we've changed given-name to Skip.
                "given-name": "Skip",
                "family-name": "Jones",
                "country": "NZ",
            },
        ],
        "remote": {
            // Remotely, we've changed the country to AU.
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "country": "AU",
        },
        "reconciled": {
            "given-name": "Skip",
            "family-name": "Jones",
            "country": "AU",
        },
    },
    {
        "description": "Multiple local changes",
        "parent": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "tel": "123456",
        },
        "local": [
            {
                "given-name": "Skip",
                "family-name": "Jones",
            },
            {
                "given-name": "Skip",
                "family-name": "Jones",
                "organization": "Mozilla",
            },
        ],
        "remote": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "tel": "123456",
            "country": "AU",
        },
        "reconciled": {
            "given-name": "Skip",
            "family-name": "Jones",
            "organization": "Mozilla",
            "country": "AU",
        },
    },
    {
        // Local and remote diverged from the shared parent, but the values are the
        // same, so we shouldn't fork.
        "description": "Same change to local and remote",
        "parent": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
        },
        "local": [
            {
                "given-name": "Skip",
                "family-name": "Jones",
            },
        ],
        "remote": {
            "version": 1,
            "given-name": "Skip",
            "family-name": "Jones",
            },
        "reconciled": {
            "given-name": "Skip",
            "family-name": "Jones",
        },
    },
    {
        "description": "Conflicting changes to single field",
        "parent": {
            // This is what we last wrote to the sync server.
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
        },
        "local": [
            {
                // The current version of the local record - the given-name has changed locally.
                "given-name": "Skip",
                "family-name": "Jones",
            },
        ],
        "remote": {
            // An incoming record has a different given-name than any of the above!
            "version": 1,
            "given-name": "Kip",
            "family-name": "Jones",
        },
        "forked": {
            // So we've forked the local record to a new GUID (and the next sync is
            // going to write this as a new record)
            "given-name": "Skip",
            "family-name": "Jones",
        },
        "reconciled": {
            // And we've updated the local version of the record to be the remote version.
            "given-name": "Kip",
            "family-name": "Jones",
        },
    },
    {
        "description": "Conflicting changes to multiple fields",
        "parent": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "country": "NZ",
        },
        "local": [
            {
                "given-name": "Skip",
                "family-name": "Jones",
                "country": "AU",
            },
        ],
        "remote": {
            "version": 1,
            "given-name": "Kip",
            "family-name": "Jones",
            "country": "CA",
        },
        "forked": {
            "given-name": "Skip",
            "family-name": "Jones",
            "country": "AU",
        },
        "reconciled": {
            "given-name": "Kip",
            "family-name": "Jones",
            "country": "CA",
        },
    },
    {
        "description": "Field deleted locally, changed remotely",
        "parent": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "country": "AU",
        },
        "local": [
            {
                "given-name": "Mark",
                "family-name": "Jones",
            },
        ],
        "remote": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "country": "NZ",
        },
        "forked": {
            "given-name": "Mark",
            "family-name": "Jones",
        },
        "reconciled": {
            "given-name": "Mark",
            "family-name": "Jones",
            "country": "NZ",
        },
    },
    {
        "description": "Field changed locally, deleted remotely",
        "parent": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "country": "AU",
        },
        "local": [
            {
                "given-name": "Mark",
                "family-name": "Jones",
                "country": "NZ",
            },
        ],
        "remote": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
        },
        "forked": {
            "given-name": "Mark",
            "family-name": "Jones",
            "country": "NZ",
        },
        "reconciled": {
            "given-name": "Mark",
            "family-name": "Jones",
        },
    },
    {
        // Created, last modified should be synced; last used and times used should
        // be local. Remote created time older than local, remote modified time
        // newer than local.
        "description": "Created, last modified time reconciliation without local changes",
        "parent": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "timeCreated": 1234,
            "timeLastModified": 5678,
            "timeLastUsed": 5678,
            "timesUsed": 6,
        },
        "local": [],
        "remote": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "timeCreated": 1200,
            "timeLastModified": 5700,
            "timeLastUsed": 5700,
            "timesUsed": 3,
        },
        "reconciled": {
            "given-name": "Mark",
            "family-name": "Jones",
            "timeCreated": 1200,
            "timeLastModified": 5700,
            // XXX - desktop has `"timeLastUsed": 5678,` which seems wrong -
            // surely the incoming later timestamp of 5700 should be used?
            "timeLastUsed": 5700,
            // Desktop has `"timesUsed": 6,` here, which is arguably correct,
            // but we don't handle this case - an item in the mirror being
            // updated when we don't have a local record isn't something that
            // can happen in practice, so we don't bother merging metadata
            // in that case - we just do the insert of the incoming.
            "timesUsed": 3,
        },
    },
    {
        // Local changes, remote created time newer than local, remote modified time
        // older than local.
        "description": "Created, last modified time reconciliation with local changes",
        "parent": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "timeCreated": 1234,
            "timeLastModified": 5678,
            "timeLastUsed": 5678,
            "timesUsed": 6,
        },
        "local": [
            {
                "given-name": "Skip",
                "family-name": "Jones",
                // desktop didn't have this metadata for local, but we need it
                // as otherwise we take ::now()
                // Further, we don't quite use the parent in the same way, so we
                // need our local record to have the same values as the parent except
                // for what's explicitly changed - which is only `given-name`.
                "timeCreated": 1234,
                "timeLastModified": 5678,
                "timeLastUsed": 5678,
                "timesUsed": 6,
            },
        ],
        "remote": {
            "version": 1,
            "given-name": "Mark",
            "family-name": "Jones",
            "timeCreated": 1300,
            "timeLastModified": 5000,
            "timeLastUsed": 5000,
            "timesUsed": 3,
        },
        "reconciled": {
            "given-name": "Skip",
            "family-name": "Jones",
            "timeCreated": 1234,
            "timeLastUsed": 5678,
            "timesUsed": 6,
        },
    }]);
}
// NOTE: test_reconcile.js also has CREDIT_CARD_RECONCILE_TESTCASES which
// we should also do.

// Takes the JSON from one of the tests above and turns it into a sync15 payload,
// suitable for sticking in the mirror or passing to the sync impl.
fn test_to_payload(guid: &SyncGuid, test_payload: &serde_json::Value) -> sync15::Payload {
    let json = json!({
        "id": guid.clone(),
        "entry": test_payload.clone(),
    });
    sync15::Payload::from_json(json).unwrap()
}

fn check_address_as_expected(address: &InternalAddress, expected: &Map<String, Value>) {
    // InternalAddress doesn't derive Serialize making this a bit painful.
    // 'expected' only has some fields, so we check them individually and explicitly.
    for (name, val) in expected.iter() {
        let name = name.as_ref();
        match name {
            "given-name" => assert_eq!(val.as_str().unwrap(), address.given_name),
            "family-name" => assert_eq!(val.as_str().unwrap(), address.family_name),
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
            _ => unreachable!("unexpected field {name}"),
        }
    }
}

// Make a local record, flagged as "changed", from the JSON in our test cases.
fn make_local_from_json(guid: &SyncGuid, json: &serde_json::Value) -> InternalAddress {
    InternalAddress {
        guid: guid.clone(),
        // Note that our test cases only include a subset of possible fields.
        given_name: json["given-name"].as_str().unwrap_or_default().to_string(),
        family_name: json["family-name"].as_str().unwrap_or_default().to_string(),
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
    let payload = test_to_payload(guid, test_payload);
    conn.execute(
        "INSERT OR IGNORE INTO addresses_mirror (guid, payload)
         VALUES (:guid, :payload)",
        rusqlite::named_params! {
            ":guid": guid,
            ":payload": payload.into_json_string(),
        },
    )
    .expect("should insert");
}

#[test]
fn test_reconcile_addresses() -> Result<()> {
    let _ = env_logger::try_init();

    let j = &ADDRESS_RECONCILE_TESTCASES;
    for test_case in j.as_array().unwrap() {
        let desc = test_case["description"].as_str().unwrap();
        let store = Arc::new(Store::new_memory());
        let db = store.db.lock().unwrap();
        let tx = db.unchecked_transaction().unwrap();

        create_empty_sync_temp_tables(&tx)?;
        log::info!("starting test case: {}", desc);
        // stick the local records in the local DB as real items.
        // Note that some test-cases have multiple "local" records, but that's
        // to explicitly test desktop's version of the "mirror", and doesn't
        // make sense here - we just want the last one.
        let local_array = test_case["local"].as_array().unwrap();
        let guid = if local_array.is_empty() {
            // no local record in this test case, so allocate a random guid.
            log::trace!("local record: doesn't exist");
            SyncGuid::random()
        } else {
            let local = local_array.last().unwrap();
            log::trace!("local record: {local}");
            let guid = SyncGuid::random();
            addresses::add_internal_address(&tx, &make_local_from_json(&guid, local))?;
            guid
        };

        // stick the "parent" item in the mirror
        let mut parent_json = test_case["parent"].clone();
        // we need to add an 'id' entry, the same as the local item we added.
        let map = parent_json.as_object_mut().unwrap();
        map.insert("id".to_string(), serde_json::to_value(guid.clone())?);
        log::trace!("parent record: {:?}", parent_json);
        insert_mirror_record(&tx, &guid, &parent_json);

        tx.commit().expect("should commit");

        // convert "incoming" items into payloads and have the sync engine apply them.
        let mut remote = test_case["remote"].clone();
        log::trace!("remote record: {:?}", remote);
        // we need to add an 'id' entry, the same as the local item we added.
        let map = remote.as_object_mut().unwrap();
        map.insert("id".to_string(), serde_json::to_value(guid.clone())?);

        let payload = test_to_payload(&guid, &remote);
        let remote_time = ServerTimestamp(0);
        let mut incoming = IncomingChangeset::new("test".to_string(), remote_time);
        incoming.changes.push((payload, ServerTimestamp(0)));

        let mut telem = telemetry::Engine::new("addresses");

        std::mem::drop(db); // unlock the mutex for the engine.
        let engine = create_address_engine(Arc::clone(&store));

        engine
            .apply_incoming(vec![incoming], &mut telem)
            .expect("should apply");

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
    }
    Ok(())
}

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
use crate::sync::address::incoming::{stage_incoming, AddressesImpl};
use crate::sync::address::AddressRecord;
use crate::sync::{do_incoming, SyncGuid};
use crate::UpdatableAddressFields;
use crate::{InternalAddress, Store};

use interrupt_support::Interruptee;
use interrupt_support::NeverInterrupts;
use rusqlite::{types::ToSql, Connection};
use serde_json::{json, Map, Value};
use sync15::Payload;

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
                "timeLastModified": 1,
                "timeLastUsed": 1,
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

/// This may (or may not :) end up in the main code at some stage. We need an
/// alternative for tombstones?
fn save_to_mirror(
    conn: &Connection,
    records: Vec<AddressRecord>,
    signal: &dyn Interruptee,
) -> Result<()> {
    log::info!("adding {} records to the mirror", records.len());

    let chunk_size = 2;
    sql_support::each_sized_chunk(
        &records,
        sql_support::default_max_variable_number() / chunk_size,
        |chunk, _| -> Result<()> {
            signal.err_if_interrupted()?;
            let sql =
                "INSERT OR REPLACE INTO addresses_mirror (guid, payload)
                 VALUES {}",
            );
            let mut params = Vec::with_capacity(chunk.len() * chunk_size);
            for record in chunk {
                let payload = record.to_value()?;
                params.push(&record.guid as &dyn ToSql);
                params.push(&payload);
            }
            conn.execute(&sql, &params)?;
            Ok(())
        },
    )
}

// The metadata fields like `time_created` etc aren't able to be set to
// specific values by our public model, so we update the DB directly.
fn update_metadata(conn: &Connection, guid: &SyncGuid, val: &Value) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for (json_name, col_name) in &[
        ("timeCreated", "time_created"),
        ("timeLastUsed", "time_last_used"),
        ("timeLastModified", "time_last_modified"),
        ("timesUsed", "times_used"),
    ] {
        if let Some(val) = val.get(json_name) {
            let sql = format!(
                "UPDATE addresses_data SET {} = :value WHERE guid = :guid",
                col_name
            );

            log::debug!("Updating metadata {} -> {:?}", col_name, val);
            tx.execute_named(
                &sql,
                rusqlite::named_params! {
                    ":guid": guid,
                    ":value": val.as_i64().unwrap(),
                },
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn check_address_as_expected(address: &InternalAddress, expected: &Map<String, Value>) {
    // 'expected' only has some fields, so to make life easy we do the final
    // comparison via json.
    let actual_json = serde_json::to_value(address).expect("should get json");
    for (name, val) in expected.iter() {
        assert_eq!(
            actual_json.get(name).unwrap(),
            val,
            "checking field '{}'",
            name
        );
    }
}

#[test]
fn test_reconcile_addresses() -> Result<()> {
    let _ = env_logger::try_init();

    let j = &ADDRESS_RECONCILE_TESTCASES;
    for test_case in j.as_array().unwrap() {
        let desc = test_case["description"].as_str().unwrap();
        let store = Store::new_memory(&format!("test_reconcile-{}", desc))?;
        let db = store.db();

        create_empty_sync_temp_tables(db)?;
        log::info!("starting test case: {}", desc);
        // stick the local records in the local DB as real items.
        // Note that some test-cases have multiple "local" records, but that's
        // to explicitly test desktop's version of the "mirror", and doesn't
        // make sense here - we just want the last one.
        let local_array = test_case["local"].as_array().unwrap();
        let guid = if local_array.is_empty() {
            // no local record in this test case, so allocate a random guid.
            SyncGuid::random()
        } else {
            let local = local_array.get(local_array.len() - 1).unwrap().clone();
            let address: UpdatableAddressFields = serde_json::from_value(local.clone()).unwrap();
            let added = store.add_address(address)?;
            update_metadata(&db, &SyncGuid::new(&added.guid), &local)?;
            SyncGuid::new(&added.guid)
        };

        // stick "incoming" (aka "remote") items in the "staging" table via a sync15 payload.
        let mut remote = test_case["remote"].clone();
        // we need to add an 'id' entry, the same as the local item we added.
        let map = remote.as_object_mut().unwrap();
        map.insert("id".to_string(), serde_json::to_value(guid.clone())?);
        let payload = Payload::from_json(remote)?;
        log::debug!("staging {:?}", payload);
        stage_incoming(db, vec![payload], &NeverInterrupts)?;

        // and finally, "parent" items are added to the mirror.
        let mut parent: AddressRecord = serde_json::from_value(test_case["parent"].clone())?;
        parent.guid = guid.clone();
        log::trace!("parent record: {:?}", parent);
        save_to_mirror(db, vec![parent], &NeverInterrupts)?;

        // OK, see what pops out!
        do_incoming(db, &AddressesImpl {}, &NeverInterrupts)?;

        let all = addresses::get_all_addresses(&db)?;
        log::info!("local records ended up as: {:#?}", all);

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

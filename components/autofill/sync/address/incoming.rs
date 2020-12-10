/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use interrupt_support::Interruptee;
use rusqlite::Transaction;
use sync15::{
    CollSyncIds, CollectionRequest, IncomingChangeset, OutgoingChangeset, Payload,
    ServerTimestamp, Store, StoreSyncAssociation, telemetry,
};
use super::{Record, RecordData};

pub fn stage_incoming(
    tx: &Transaction<'_>,
    incoming_payloads: Vec<Payload>,
    signal: &dyn Interruptee,
) -> Result<()> {
    let mut incoming_records = Vec::with_capacity(incoming_payloads.len());
    for payload in incoming_payloads {
        incoming_records.push(payload.into_record::<AddressRecord>()?);
    }
    let chunk_size = 3;
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
                    email,
                ) VALUES {}",
                sql_support::repeat_multi_values(chunk.len(), chunk_size)
            );
            let mut params = Vec::with_capacity(chunk.len() * chunk_size);
            for record in chunk {
                signal.err_if_interrupted()?;
                params.push(&record.guid as &dyn ToSql);
                match &record.data {
                    AddressRecordData::Data {
                        ref given_name,
                        ref additional_name,
                        ref family_name,
                        ref organization,
                        ref street_address,
                        ref address_level3,
                        ref address_level2,
                        ref address_level1,
                        ref postal_code,
                        ref country,
                        ref tel,
                        ref email,
                    } => {
                        params.push(given_name);
                        params.push(additional_name);
                        params.push(family_name);
                        params.push(organization);
                        params.push(street_address);
                        params.push(address_level3);
                        params.push(address_level2);
                        params.push(address_level1);
                        params.push(postal_code);
                        params.push(country);
                        params.push(tel);
                        params.push(email);
                    }
                    AddressRecordData::Tombstone => {
                        params.push(&Null);
                        params.push(&Null);
                        params.push(&Null);
                        params.push(&Null);
                        params.push(&Null);
                        params.push(&Null);
                        params.push(&Null);
                        params.push(&Null);
                        params.push(&Null);
                        params.push(&Null);
                        params.push(&Null);
                        params.push(&Null);
                    }
                }
            }
            tx.execute(&sql, &params)?;
            Ok(())
        },
    )?;
    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum IncomingState {
    IncomingOnly { guid: String, incoming: RecordData },
    IncomingTombstone { guid: String },
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
    }
    LocalTombstone { guid: String }
    NoLocal {
        guid: String,
        incoming: RecordData,
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
        IncomingState::IncomingTombstone => IncomingAction::DeleteLocally,
        IncomingState::HasLocal { guid, incoming, local, mirror} => IncomingAction::MergeLocal,
        IncomingState::HasLocalDupe {
            guid,
            dupe_guid,
            dupe,
            incoming,
            mirror
        } => IncomingAction::MergeDupe,
        // It might be better for the `LocalTombstone` state to perform the `DeleteLocally` action.
        // But I think since the store's delete action remove the record from the local table
        // and adds it to the store in a transaction an inconsistent state would be virtually
        // impossible. Also this might require updating the sync counter *if* we add that column
        // to the tombstones table.
        IncomingState::LocalTombstone { guid } => IncomingAction::Nothing,
        IncomingState::NoLocal { guid, incoming } => IncomingAction::TakeRemote,
    }
}

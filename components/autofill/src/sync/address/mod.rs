/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod incoming;
pub mod outgoing;

use super::engine::{ConfigSyncEngine, EngineConfig, SyncEngineStorageImpl};
use super::{
    MergeResult, Metadata, ProcessIncomingRecordImpl, ProcessOutgoingRecordImpl, SyncRecord,
};
use crate::db::models::address::InternalAddress;
use crate::error::*;
use crate::sync_merge_field_check;
use incoming::IncomingAddressesImpl;
use outgoing::OutgoingAddressesImpl;
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sync_guid::Guid;
use types::Timestamp;

// The engine.
pub(crate) fn create_engine(store: Arc<crate::Store>) -> ConfigSyncEngine<InternalAddress> {
    ConfigSyncEngine::new(
        EngineConfig {
            namespace: "addresses".to_string(),
            collection: "addresses",
        },
        store,
        Box::new(AddressesEngineStorageImpl {}),
    )
}

pub(super) struct AddressesEngineStorageImpl {}

impl SyncEngineStorageImpl<InternalAddress> for AddressesEngineStorageImpl {
    fn get_incoming_impl(
        &self,
        enc_key: &Option<String>,
    ) -> Result<Box<dyn ProcessIncomingRecordImpl<Record = InternalAddress>>> {
        assert!(enc_key.is_none());
        Ok(Box::new(IncomingAddressesImpl {}))
    }

    fn reset_storage(&self, tx: &Transaction<'_>) -> Result<()> {
        tx.execute_batch(
            "DELETE FROM addresses_mirror;
            DELETE FROM addresses_tombstones;",
        )?;
        Ok(())
    }

    fn get_outgoing_impl(
        &self,
        enc_key: &Option<String>,
    ) -> Result<Box<dyn ProcessOutgoingRecordImpl<Record = InternalAddress>>> {
        assert!(enc_key.is_none());
        Ok(Box::new(OutgoingAddressesImpl {}))
    }
}

// These structs are what's stored on the sync server.
#[derive(Default, Deserialize, Serialize)]
struct AddressPayload {
    id: Guid,
    // For some historical reason and unlike most other sync records, addresses
    // are serialized with this explicit 'entry' object.
    entry: PayloadEntry,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
struct PayloadEntry {
    pub given_name: String,
    pub additional_name: String,
    pub family_name: String,
    pub organization: String,
    pub street_address: String,
    pub address_level3: String,
    pub address_level2: String,
    pub address_level1: String,
    pub postal_code: String,
    pub country: String,
    pub tel: String,
    pub email: String,
    // metadata (which isn't kebab-case for some historical reason...)
    #[serde(rename = "timeCreated")]
    pub time_created: Timestamp,
    #[serde(rename = "timeLastUsed")]
    pub time_last_used: Timestamp,
    #[serde(rename = "timeLastModified")]
    pub time_last_modified: Timestamp,
    #[serde(rename = "timesUsed")]
    pub times_used: i64,
    pub version: u32, // always 3 for credit-cards
}

impl InternalAddress {
    fn from_payload(sync_payload: sync15::Payload) -> Result<Self> {
        let p: AddressPayload = sync_payload.into_record()?;
        if p.entry.version != 1 {
            // Always been version 1
            return Err(Error::InvalidSyncPayload(format!(
                "invalid version - {}",
                p.entry.version
            )));
        }

        Ok(InternalAddress {
            guid: p.id,
            given_name: p.entry.given_name,
            additional_name: p.entry.additional_name,
            family_name: p.entry.family_name,
            organization: p.entry.organization,
            street_address: p.entry.street_address,
            address_level3: p.entry.address_level3,
            address_level2: p.entry.address_level2,
            address_level1: p.entry.address_level1,
            postal_code: p.entry.postal_code,
            country: p.entry.country,
            tel: p.entry.tel,
            email: p.entry.email,
            metadata: Metadata {
                time_created: p.entry.time_created,
                time_last_used: p.entry.time_last_used,
                time_last_modified: p.entry.time_last_modified,
                times_used: p.entry.times_used,
                sync_change_counter: 0,
            },
        })
    }

    pub fn into_payload(self) -> Result<sync15::Payload> {
        let p = AddressPayload {
            id: self.guid,
            entry: PayloadEntry {
                given_name: self.given_name,
                additional_name: self.additional_name,
                family_name: self.family_name,
                organization: self.organization,
                street_address: self.street_address,
                address_level3: self.address_level3,
                address_level2: self.address_level2,
                address_level1: self.address_level1,
                postal_code: self.postal_code,
                country: self.country,
                tel: self.tel,
                email: self.email,
                time_created: self.metadata.time_created,
                time_last_used: self.metadata.time_last_used,
                time_last_modified: self.metadata.time_last_modified,
                times_used: self.metadata.times_used,
                version: 1,
            },
        };
        Ok(sync15::Payload::from_record(p)?)
    }
}

impl SyncRecord for InternalAddress {
    fn record_name() -> &'static str {
        "Address"
    }

    fn id(&self) -> &Guid {
        &self.guid
    }

    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }

    /// Performs a three-way merge between an incoming, local, and mirror record.
    /// If a merge cannot be successfully completed (ie, if we find the same
    /// field has changed both locally and remotely since the last sync), the
    /// local record data is returned with a new guid and updated sync metadata.
    /// Note that mirror being None is an edge-case and typically means first
    /// sync since a "reset" (eg, disconnecting and reconnecting.
    #[allow(clippy::cognitive_complexity)] // Looks like clippy considers this after macro-expansion...
    fn merge(incoming: &Self, local: &Self, mirror: &Option<Self>) -> MergeResult<Self> {
        let mut merged_record: Self = Default::default();
        // guids must be identical
        assert_eq!(incoming.guid, local.guid);

        match mirror {
            Some(m) => assert_eq!(incoming.guid, m.guid),
            None => {}
        };

        merged_record.guid = incoming.guid.clone();

        sync_merge_field_check!(given_name, incoming, local, mirror, merged_record);
        sync_merge_field_check!(additional_name, incoming, local, mirror, merged_record);
        sync_merge_field_check!(family_name, incoming, local, mirror, merged_record);
        sync_merge_field_check!(organization, incoming, local, mirror, merged_record);
        sync_merge_field_check!(street_address, incoming, local, mirror, merged_record);
        sync_merge_field_check!(address_level3, incoming, local, mirror, merged_record);
        sync_merge_field_check!(address_level2, incoming, local, mirror, merged_record);
        sync_merge_field_check!(address_level1, incoming, local, mirror, merged_record);
        sync_merge_field_check!(postal_code, incoming, local, mirror, merged_record);
        sync_merge_field_check!(country, incoming, local, mirror, merged_record);
        sync_merge_field_check!(tel, incoming, local, mirror, merged_record);
        sync_merge_field_check!(email, incoming, local, mirror, merged_record);

        merged_record.metadata = incoming.metadata;
        merged_record
            .metadata
            .merge(&local.metadata, mirror.as_ref().map(|m| m.metadata()));

        MergeResult::Merged {
            merged: merged_record,
        }
    }
}

/// Returns a with the given local record's data but with a new guid and
/// fresh sync metadata.
fn get_forked_record(local_record: InternalAddress) -> InternalAddress {
    let mut local_record_data = local_record;
    local_record_data.guid = Guid::random();
    local_record_data.metadata.time_created = Timestamp::now();
    local_record_data.metadata.time_last_used = Timestamp::now();
    local_record_data.metadata.time_last_modified = Timestamp::now();
    local_record_data.metadata.times_used = 0;
    local_record_data.metadata.sync_change_counter = 1;

    local_record_data
}

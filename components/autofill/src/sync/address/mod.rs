/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod incoming;

use super::engine::{ConfigSyncEngine, EngineConfig, SyncEngineStorageImpl};
use super::{MergeResult, Metadata, ProcessIncomingRecordImpl, SyncRecord};
use crate::db::models::address::InternalAddress;
use crate::error::*;
use crate::sync_merge_field_check;
use incoming::AddressesImpl;
use rusqlite::Transaction;
use sync_guid::Guid as SyncGuid;
use types::Timestamp;

// The engine.
#[allow(dead_code)]
pub(super) fn get_engine(db: &'_ crate::db::AutofillDb) -> ConfigSyncEngine<'_, InternalAddress> {
    ConfigSyncEngine {
        db,
        config: EngineConfig {
            namespace: "addresses".to_string(),
            collection: "addresses",
        },
        storage_impl: Box::new(AddressesEngineStorageImpl {}),
    }
}

pub(super) struct AddressesEngineStorageImpl {}

impl SyncEngineStorageImpl<InternalAddress> for AddressesEngineStorageImpl {
    fn get_incoming_impl(&self) -> Box<dyn ProcessIncomingRecordImpl<Record = InternalAddress>> {
        Box::new(AddressesImpl {})
    }

    fn reset_storage(&self, tx: &Transaction<'_>) -> Result<()> {
        tx.execute_batch(
            "DELETE FROM addresses_mirror;
            DELETE FROM addresses_tombstones;
            UPDATE addresses_data SET sync_change_counter = 1",
        )?;
        Ok(())
    }
}

impl SyncRecord for InternalAddress {
    fn record_name() -> &'static str {
        "Address"
    }

    fn id(&self) -> &SyncGuid {
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
            .merge(&local.metadata, &mirror.as_ref().map(|m| m.metadata()));

        MergeResult::Merged {
            merged: merged_record,
        }
    }
}

/// Returns a with the given local record's data but with a new guid and
/// fresh sync metadata.
fn get_forked_record(local_record: InternalAddress) -> InternalAddress {
    let mut local_record_data = local_record;
    local_record_data.guid = SyncGuid::random();
    local_record_data.metadata.time_created = Timestamp::now();
    local_record_data.metadata.time_last_used = Timestamp::now();
    local_record_data.metadata.time_last_modified = Timestamp::now();
    local_record_data.metadata.times_used = 0;
    local_record_data.metadata.sync_change_counter = 1;

    local_record_data
}

/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod incoming;

use super::engine::{ConfigSyncEngine, EngineConfig, SyncEngineStorageImpl};
use super::{MergeResult, Metadata, ProcessIncomingRecordImpl, SyncRecord};
use crate::db::models::credit_card::InternalCreditCard;
use crate::error::*;
use crate::sync_merge_field_check;
use incoming::CreditCardsImpl;
use rusqlite::Transaction;
use sync_guid::Guid as SyncGuid;
use types::Timestamp;

// The engine.
#[allow(dead_code)]
pub(super) fn get_engine(
    db: &'_ crate::db::AutofillDb,
) -> ConfigSyncEngine<'_, InternalCreditCard> {
    ConfigSyncEngine {
        db,
        config: EngineConfig {
            namespace: "credit_cards".to_string(),
            collection: "creditcards",
        },
        storage_impl: Box::new(CreditCardsEngineStorageImpl {}),
    }
}

pub(super) struct CreditCardsEngineStorageImpl {}

impl SyncEngineStorageImpl<InternalCreditCard> for CreditCardsEngineStorageImpl {
    fn get_incoming_impl(&self) -> Box<dyn ProcessIncomingRecordImpl<Record = InternalCreditCard>> {
        Box::new(CreditCardsImpl {})
    }

    fn reset_storage(&self, tx: &Transaction<'_>) -> Result<()> {
        tx.execute_batch(
            "DELETE FROM credit_cards_mirror;
            DELETE FROM credit_cards_tombstones;
            UPDATE credit_cards_data SET sync_change_counter = 1",
        )?;
        Ok(())
    }
}

impl SyncRecord for InternalCreditCard {
    fn record_name() -> &'static str {
        "CreditCard"
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

        sync_merge_field_check!(cc_name, incoming, local, mirror, merged_record);
        sync_merge_field_check!(cc_number, incoming, local, mirror, merged_record);
        sync_merge_field_check!(cc_exp_month, incoming, local, mirror, merged_record);
        sync_merge_field_check!(cc_exp_year, incoming, local, mirror, merged_record);
        sync_merge_field_check!(cc_type, incoming, local, mirror, merged_record);

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
fn get_forked_record(local_record: InternalCreditCard) -> InternalCreditCard {
    let mut local_record_data = local_record;
    local_record_data.guid = SyncGuid::random();
    local_record_data.metadata.time_created = Timestamp::now();
    local_record_data.metadata.time_last_used = Timestamp::now();
    local_record_data.metadata.time_last_modified = Timestamp::now();
    local_record_data.metadata.times_used = 0;
    local_record_data.metadata.sync_change_counter = 1;

    local_record_data
}

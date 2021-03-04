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
use crate::db::models::credit_card::InternalCreditCard;
use crate::error::*;
use crate::sync_merge_field_check;
use incoming::IncomingCreditCardsImpl;
use outgoing::OutgoingCreditCardsImpl;
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use sync_guid::Guid;
use types::Timestamp;

// The engine.
pub fn create_engine(
    db: Arc<Mutex<crate::db::AutofillDb>>,
) -> ConfigSyncEngine<InternalCreditCard> {
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
        Box::new(IncomingCreditCardsImpl {})
    }

    fn reset_storage(&self, tx: &Transaction<'_>) -> Result<()> {
        tx.execute_batch(
            "DELETE FROM credit_cards_mirror;
            DELETE FROM credit_cards_tombstones;",
        )?;
        Ok(())
    }

    fn get_outgoing_impl(&self) -> Box<dyn ProcessOutgoingRecordImpl<Record = InternalCreditCard>> {
        Box::new(OutgoingCreditCardsImpl {})
    }
}

// These structs are what's stored on the sync server.
#[derive(Default, Debug, Deserialize, Serialize)]
struct CreditCardPayload {
    id: Guid,
    // For some historical reason and unlike most other sync records, creditcards
    // are serialized with this explicit 'entry' object.
    entry: PayloadEntry,
}

#[derive(Default, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
struct PayloadEntry {
    pub cc_name: String,
    pub cc_number: String,
    pub cc_exp_month: i64,
    pub cc_exp_year: i64,
    pub cc_type: String,
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

impl InternalCreditCard {
    fn from_payload(sync_payload: sync15::Payload) -> Result<Self> {
        let p: CreditCardPayload = sync_payload.into_record()?;
        if p.entry.version != 3 {
            // when new versions are introduced we will start accepting and
            // converting old ones - but 3 is the lowest we support.
            return Err(Error::InvalidSyncPayload(format!(
                "invalid version - {}",
                p.entry.version
            )));
        }
        Ok(InternalCreditCard {
            guid: p.id,
            cc_name: p.entry.cc_name,
            cc_number: p.entry.cc_number,
            cc_exp_month: p.entry.cc_exp_month,
            cc_exp_year: p.entry.cc_exp_year,
            cc_type: p.entry.cc_type,
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
        let p = CreditCardPayload {
            id: self.guid,
            entry: PayloadEntry {
                cc_name: self.cc_name,
                cc_number: self.cc_number,
                cc_exp_month: self.cc_exp_month,
                cc_exp_year: self.cc_exp_year,
                cc_type: self.cc_type,
                time_created: self.metadata.time_created,
                time_last_used: self.metadata.time_last_used,
                time_last_modified: self.metadata.time_last_modified,
                times_used: self.metadata.times_used,
                version: 3,
            },
        };
        Ok(sync15::Payload::from_record(p)?)
    }
}

impl SyncRecord for InternalCreditCard {
    fn record_name() -> &'static str {
        "CreditCard"
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

        sync_merge_field_check!(cc_name, incoming, local, mirror, merged_record);
        sync_merge_field_check!(cc_number, incoming, local, mirror, merged_record);
        sync_merge_field_check!(cc_exp_month, incoming, local, mirror, merged_record);
        sync_merge_field_check!(cc_exp_year, incoming, local, mirror, merged_record);
        sync_merge_field_check!(cc_type, incoming, local, mirror, merged_record);

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
fn get_forked_record(local_record: InternalCreditCard) -> InternalCreditCard {
    let mut local_record_data = local_record;
    local_record_data.guid = Guid::random();
    local_record_data.metadata.time_created = Timestamp::now();
    local_record_data.metadata.time_last_used = Timestamp::now();
    local_record_data.metadata.time_last_modified = Timestamp::now();
    local_record_data.metadata.times_used = 0;
    local_record_data.metadata.sync_change_counter = 1;

    local_record_data
}

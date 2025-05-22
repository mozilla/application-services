/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod incoming;
pub mod outgoing;

use super::engine::{ConfigSyncEngine, EngineConfig, SyncEngineStorageImpl};
use super::{
    MergeResult, Metadata, ProcessIncomingRecordImpl, ProcessOutgoingRecordImpl, SyncRecord,
    UnknownFields,
};
use crate::db::models::credit_card::InternalCreditCard;
use crate::encryption::EncryptorDecryptor;
use crate::error::*;
use crate::sync_merge_field_check;
use incoming::IncomingCreditCardsImpl;
use outgoing::OutgoingCreditCardsImpl;
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use sync_guid::Guid;
use types::Timestamp;

// The engine.
pub(crate) fn create_engine(store: Arc<crate::Store>) -> ConfigSyncEngine<InternalCreditCard> {
    ConfigSyncEngine::new(
        EngineConfig {
            namespace: "credit_cards".to_string(),
            collection: "creditcards".into(),
        },
        store,
        Box::new(CreditCardsEngineStorageImpl {}),
    )
}

pub(super) struct CreditCardsEngineStorageImpl {}

impl SyncEngineStorageImpl<InternalCreditCard> for CreditCardsEngineStorageImpl {
    fn get_incoming_impl(
        &self,
        enc_key: &Option<String>,
    ) -> Result<Box<dyn ProcessIncomingRecordImpl<Record = InternalCreditCard>>> {
        let enc_key = match enc_key {
            None => return Err(Error::MissingEncryptionKey),
            Some(enc_key) => enc_key,
        };
        let encdec = EncryptorDecryptor::new(enc_key)?;
        Ok(Box::new(IncomingCreditCardsImpl { encdec }))
    }

    fn reset_storage(&self, tx: &Transaction<'_>) -> Result<()> {
        tx.execute_batch(
            "DELETE FROM credit_cards_mirror;
            DELETE FROM credit_cards_tombstones;",
        )?;
        Ok(())
    }

    fn get_outgoing_impl(
        &self,
        enc_key: &Option<String>,
    ) -> Result<Box<dyn ProcessOutgoingRecordImpl<Record = InternalCreditCard>>> {
        let enc_key = match enc_key {
            None => return Err(Error::MissingEncryptionKey),
            Some(enc_key) => enc_key,
        };
        let encdec = EncryptorDecryptor::new(enc_key)?;
        Ok(Box::new(OutgoingCreditCardsImpl { encdec }))
    }
}

// These structs are a representation of what's stored on the sync server for non-tombstone records.
// (The actual server doesn't have `id` in the payload but instead in the envelope)
#[derive(Default, Debug, Deserialize, Serialize)]
pub(crate) struct CreditCardPayload {
    id: Guid,

    // For some historical reason and unlike most other sync records, creditcards
    // are serialized with this explicit 'entry' object.
    pub(super) entry: PayloadEntry,
}

// Note that the sync payload contains the "unencrypted" cc_number - but our
// internal structs have the cc_number_enc/cc_number_last_4 pair, so we need to
// take care going to and from.
// (The scare-quotes around "unencrypted" are to reflect that, obviously, the
// payload itself *is* encrypted by sync, but the number is plain-text in that
// payload)
#[derive(Default, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case")]
pub(super) struct PayloadEntry {
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
    // Fields that the current schema did not expect, we store them only internally
    // to round-trip them back to sync without processing them in any way
    #[serde(flatten)]
    pub unknown_fields: UnknownFields,
}

impl InternalCreditCard {
    fn from_payload(p: CreditCardPayload, encdec: &EncryptorDecryptor) -> Result<Self> {
        if p.entry.version != 3 {
            // when new versions are introduced we will start accepting and
            // converting old ones - but 3 is the lowest we support.
            return Err(Error::InvalidSyncPayload(format!(
                "invalid version - {}",
                p.entry.version
            )));
        }
        // need to encrypt the cleartext in the sync record.
        let cc_number_enc = encdec.encrypt(&p.entry.cc_number)?;
        let cc_number_last_4 = get_last_4(&p.entry.cc_number);

        Ok(InternalCreditCard {
            guid: p.id,
            cc_name: p.entry.cc_name,
            cc_number_enc,
            cc_number_last_4,
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

    pub(crate) fn into_payload(self, encdec: &EncryptorDecryptor) -> Result<CreditCardPayload> {
        let cc_number = encdec.decrypt(&self.cc_number_enc)?;
        Ok(CreditCardPayload {
            id: self.guid,
            entry: PayloadEntry {
                cc_name: self.cc_name,
                cc_number,
                cc_exp_month: self.cc_exp_month,
                cc_exp_year: self.cc_exp_year,
                cc_type: self.cc_type,
                time_created: self.metadata.time_created,
                time_last_used: self.metadata.time_last_used,
                time_last_modified: self.metadata.time_last_modified,
                times_used: self.metadata.times_used,
                version: 3,
                unknown_fields: Default::default(),
            },
        })
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

        if let Some(m) = mirror {
            assert_eq!(incoming.guid, m.guid)
        };

        merged_record.guid = incoming.guid.clone();

        sync_merge_field_check!(cc_name, incoming, local, mirror, merged_record);
        // XXX - It looks like this will allow us to merge a locally changed
        // cc_number_enc and remotely changed cc_number_last_4, which is nonsensical.
        // Given sync itself is populating this it needs more thought.
        sync_merge_field_check!(cc_number_enc, incoming, local, mirror, merged_record);
        sync_merge_field_check!(cc_number_last_4, incoming, local, mirror, merged_record);
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

// Wow - strings are hard! credit-card sync is the only thing that needs to
// get the last 4 chars of a string.
fn get_last_4(v: &str) -> String {
    v.chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>()
}
#[test]
fn test_last_4() {
    assert_eq!(get_last_4("testing"), "ting".to_string());
    assert_eq!(get_last_4("abc"), "abc".to_string());
    assert_eq!(get_last_4(""), "".to_string());
}

#[test]
fn test_to_from_payload() {
    nss::ensure_initialized();
    let key = crate::encryption::create_autofill_key().unwrap();
    let cc_number = "1234567812345678";
    let cc_number_enc =
        crate::encryption::encrypt_string(key.clone(), cc_number.to_string()).unwrap();
    let cc = InternalCreditCard {
        cc_name: "Shaggy".to_string(),
        cc_number_enc,
        cc_number_last_4: "5678".to_string(),
        cc_exp_month: 12,
        cc_exp_year: 2021,
        cc_type: "foo".to_string(),
        ..Default::default()
    };
    let encdec = EncryptorDecryptor::new(&key).unwrap();
    let payload: CreditCardPayload = cc.clone().into_payload(&encdec).unwrap();

    assert_eq!(payload.id, cc.guid);
    assert_eq!(payload.entry.cc_name, "Shaggy".to_string());
    assert_eq!(payload.entry.cc_number, cc_number.to_string());
    assert_eq!(payload.entry.cc_exp_month, 12);
    assert_eq!(payload.entry.cc_exp_year, 2021);
    assert_eq!(payload.entry.cc_type, "foo".to_string());

    // and back.
    let cc2 = InternalCreditCard::from_payload(payload, &encdec).unwrap();
    // sadly we can't just check equality because the encrypted value will be
    // different even if the card number is identical.
    assert_eq!(cc2.guid, cc.guid);
    assert_eq!(cc2.cc_name, "Shaggy".to_string());
    assert_eq!(cc2.cc_number_last_4, cc.cc_number_last_4);
    assert_eq!(cc2.cc_exp_month, cc.cc_exp_month);
    assert_eq!(cc2.cc_exp_year, cc.cc_exp_year);
    assert_eq!(cc2.cc_type, cc.cc_type);
    // The decrypted number should be the same.
    assert_eq!(
        crate::encryption::decrypt_string(key, cc2.cc_number_enc.clone()).unwrap(),
        cc_number
    );
    // But the encrypted value should not.
    assert_ne!(cc2.cc_number_enc, cc.cc_number_enc);
}

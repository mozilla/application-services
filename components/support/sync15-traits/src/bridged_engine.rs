/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::Guid;

use crate::error;

// This module hold structs which will eventually live outside of this
// module and will be re-used by bso_record. It lives here temporarily
// as we move the bridged_engine away from Payload so we can see the shape
// that this and bso_record will share.

// It is a separate module here to help split it out later.
pub mod public {
    use crate::{Guid, ServerTimestamp};
    use serde::{Deserialize, Serialize};

    /// An envelope for an incoming item. Envelopes carry all the metadata for
    /// a Sync BSO record (`id`, `modified`, `sortindex`), *but not* the payload
    /// itself.
    #[derive(Debug, Clone, Deserialize)]
    pub struct IncomingEnvelope {
        pub id: Guid,

        #[serde(skip_serializing)]
        // If we don't give it a default, we fail to deserialize
        // items we wrote out during tests and such.
        // XXX - we should probably fix the tests and  kill this.
        #[serde(default = "ServerTimestamp::default")]
        pub modified: ServerTimestamp,

        #[serde(skip_serializing_if = "Option::is_none")]
        pub sortindex: Option<i32>,

        #[serde(skip_serializing_if = "Option::is_none")]
        pub ttl: Option<u32>,
    }

    /// An envelope for an outgoing item. This is conceptually identical to
    /// [IncomingEnvelope], but omits fields that are only set by the server,
    /// like `modified`.
    #[derive(Debug, Default, Clone, Serialize, PartialEq)]
    pub struct OutgoingEnvelope {
        pub id: Guid,
        pub sortindex: Option<i32>,
        pub ttl: Option<u32>,
    }

    // Allow an envelope to be constructed with nothing but a guid.
    impl From<Guid> for OutgoingEnvelope {
        fn from(id: Guid) -> Self {
            OutgoingEnvelope {
                id,
                ..Default::default()
            }
        }
    }

    /// The result of deserializing or decrypting an incoming payload. Note that decryption failures
    /// are intentionally an error and not captured in this enum.
    /// Note we don't carry either the GUID for Tombstone, nor the raw payload for
    /// Malformed as assume the caller still owns the envelope and raw payload.
    pub enum IncomingDeserPayload<T> {
        /// A good T.
        Record(T),
        /// A tombstone
        Tombstone,
        /// Either not JSON, or can't be made into a T.
        Malformed,
    }

    impl<T> IncomingDeserPayload<T> {
        /// Returns Some(record) if [IncomingDeserPayload::Record], None otherwise.
        pub fn record(self) -> Option<T> {
            match self {
                Self::Record(t) => Some(t),
                _ => None,
            }
        }
    }
}

pub use public::{IncomingDeserPayload, IncomingEnvelope, OutgoingEnvelope};

/// A BridgedEngine acts as a bridge between application-services, rust
/// implemented sync engines and sync engines as defined by Desktop Firefox.
///
/// [Desktop Firefox has an abstract implementation of a Sync
/// Engine](https://searchfox.org/mozilla-central/source/services/sync/modules/engines.js)
/// with a number of functions each engine is expected to override. Engines
/// implemented in Rust use a different shape (specifically, the
/// [SyncEngine](crate::SyncEngine) trait), so this BridgedEngine trait adapts
/// between the 2.
pub trait BridgedEngine {
    /// The type returned for errors.
    type Error;

    /// Returns the last sync time, in milliseconds, for this engine's
    /// collection. This is called before each sync, to determine the lower
    /// bound for new records to fetch from the server.
    fn last_sync(&self) -> Result<i64, Self::Error>;

    /// Sets the last sync time, in milliseconds. This is called throughout
    /// the sync, to fast-forward the stored last sync time to match the
    /// timestamp on the uploaded records.
    fn set_last_sync(&self, last_sync_millis: i64) -> Result<(), Self::Error>;

    /// Returns the sync ID for this engine's collection. This is only used in
    /// tests.
    fn sync_id(&self) -> Result<Option<String>, Self::Error>;

    /// Resets the sync ID for this engine's collection, returning the new ID.
    /// As a side effect, implementations should reset all local Sync state,
    /// as in `reset`.
    fn reset_sync_id(&self) -> Result<String, Self::Error>;

    /// Ensures that the locally stored sync ID for this engine's collection
    /// matches the `new_sync_id` from the server. If the two don't match,
    /// implementations should reset all local Sync state, as in `reset`.
    /// This method returns the assigned sync ID, which can be either the
    /// `new_sync_id`, or a different one if the engine wants to force other
    /// devices to reset their Sync state for this collection the next time they
    /// sync.
    fn ensure_current_sync_id(&self, new_sync_id: &str) -> Result<String, Self::Error>;

    /// Indicates that the engine is about to start syncing. This is called
    /// once per sync, and always before `store_incoming`.
    fn sync_started(&self) -> Result<(), Self::Error>;

    /// Stages a batch of incoming Sync records. This is called multiple
    /// times per sync, once for each batch. Implementations can use the
    /// signal to check if the operation was aborted, and cancel any
    /// pending work.
    fn store_incoming(
        &self,
        incoming_cleartexts: &[IncomingBridgeRecord],
    ) -> Result<(), Self::Error>;

    /// Applies all staged records, reconciling changes on both sides and
    /// resolving conflicts. Returns a list of records to upload.
    fn apply(&self) -> Result<ApplyResults, Self::Error>;

    /// Indicates that the given record IDs were uploaded successfully to the
    /// server. This is called multiple times per sync, once for each batch
    /// upload.
    fn set_uploaded(&self, server_modified_millis: i64, ids: &[Guid]) -> Result<(), Self::Error>;

    /// Indicates that all records have been uploaded. At this point, any record
    /// IDs marked for upload that haven't been passed to `set_uploaded`, can be
    /// assumed to have failed: for example, because the server rejected a record
    /// with an invalid TTL or sort index.
    fn sync_finished(&self) -> Result<(), Self::Error>;

    /// Resets all local Sync state, including any change flags, mirrors, and
    /// the last sync time, such that the next sync is treated as a first sync
    /// with all new local data. Does not erase any local user data.
    fn reset(&self) -> Result<(), Self::Error>;

    /// Erases all local user data for this collection, and any Sync metadata.
    /// This method is destructive, and unused for most collections.
    fn wipe(&self) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, Default)]
pub struct ApplyResults {
    /// List of records
    pub records: Vec<OutgoingBridgeRecord>,
    /// The number of incoming records whose contents were merged because they
    /// changed on both sides. None indicates we aren't reporting this
    /// information.
    pub num_reconciled: Option<usize>,
}

impl ApplyResults {
    pub fn new(
        records: Vec<OutgoingBridgeRecord>,
        num_reconciled: impl Into<Option<usize>>,
    ) -> Self {
        Self {
            records,
            num_reconciled: num_reconciled.into(),
        }
    }
}

// Shorthand for engines that don't care.
impl From<Vec<OutgoingBridgeRecord>> for ApplyResults {
    fn from(records: Vec<OutgoingBridgeRecord>) -> Self {
        Self {
            records,
            num_reconciled: None,
        }
    }
}

/// TODO: integrate IncomingBridgeRecord and OutgoingBridgeRecord with bso_record,
/// but the decryption and cleartext semantics don't quite align, so let's look
/// at doing that later.
#[derive(Clone, Debug, Deserialize)]
pub struct IncomingBridgeRecord {
    #[serde(flatten)]
    pub envelope: IncomingEnvelope,
    // Don't provide access to the cleartext directly. We want all callers to
    // use `IncomingEnvelope::payload`, so that we can validate the cleartext
    // (where 'validate' in this context means 'let serde fail if it's invalid`)
    cleartext: String,
}

impl IncomingBridgeRecord {
    /// Parses and returns an IncomingDeserPayload describing the record.
    /// All JSON errors are treated as [IncomingDeserPayload::Malformed].
    pub fn payload<T>(&self) -> IncomingDeserPayload<T>
    where
        T: DeserializeOwned,
    {
        let mut json = match serde_json::from_str(&self.cleartext) {
            Ok(v) => v,
            Err(_) => {
                log::warn!("Invalid incoming json {}", self.envelope.id);
                return IncomingDeserPayload::Malformed;
            }
        };

        if let serde_json::Value::Object(ref map) = json {
            if map.contains_key("deleted") {
                return IncomingDeserPayload::Tombstone;
            }
        };

        // In general, the payload does not carry 'id', but <T> does - so put
        // it into the json before deserializing the record.
        if let serde_json::Value::Object(ref mut map) = json {
            match map.get("id").and_then(|v| v.as_str()) {
                Some(id) => {
                    // It exists in the payload! Note that this *should not* happen in practice
                    // (the `id` should *never* be in the payload), but if that does happen
                    // we should do the "right" thing, which is treat a mismatch as malformed.
                    if id != self.envelope.id {
                        log::warn!(
                            "malformed incoming record: envelope id: {} payload id: {}",
                            self.envelope.id,
                            id
                        );
                        return IncomingDeserPayload::Malformed;
                    }
                    if !self.envelope.id.is_valid_for_sync_server() {
                        log::warn!("malformed incoming record: id is not valid: {}", id);
                        return IncomingDeserPayload::Malformed;
                    }
                    log::warn!("incoming record has 'id' in the payload - it does match, but is still unexpected");
                }
                None => {
                    let id = &self.envelope.id;
                    if !id.is_valid_for_sync_server() {
                        log::warn!("malformed incoming record: id is not valid: {}", id);
                        return IncomingDeserPayload::Malformed;
                    }
                    map.insert("id".to_string(), id.to_string().into());
                }
            }
        }

        match serde_json::from_value::<T>(json) {
            Ok(v) => IncomingDeserPayload::Record(v),
            Err(_) => {
                log::warn!("Incoming JSON can't be turned into T: {}", self.envelope.id);
                IncomingDeserPayload::Malformed
            }
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct OutgoingBridgeRecord {
    #[serde(flatten)]
    pub envelope: OutgoingEnvelope,
    // cleartext private to force use of from_record.
    cleartext: String,
}

impl OutgoingBridgeRecord {
    pub fn new_tombstone(envelope: OutgoingEnvelope) -> Self {
        Self {
            envelope,
            cleartext: serde_json::json!({"deleted": true}).to_string(),
        }
    }

    /// Create an Outgoing record with a default envelope, where the id
    /// can be found in the record's json payload.
    pub fn from_record_with_id<T>(record: T) -> error::Result<Self>
    where
        T: Serialize,
    {
        let mut payload = serde_json::to_value(record)?;
        let envelope = match payload.as_object_mut() {
            Some(ref mut map) => {
                match map.remove("id").as_ref().and_then(|v| v.as_str()) {
                    Some(id) => {
                        let id: Guid = id.into();
                        assert!(id.is_valid_for_sync_server(), "record's ID is invalid");
                        OutgoingEnvelope {
                            id,
                            sortindex: None,
                            ttl: None,
                        }
                    }
                    // This is a "static" error and not influenced by runtime behavior
                    None => panic!("record does not have an ID in the payload"),
                }
            }
            None => panic!("record is not a json object"),
        };
        Ok(Self {
            envelope,
            cleartext: serde_json::to_string(&payload)?,
        })
    }

    /// Create an Outgoing record with an explicit envelope. Will panic if the
    /// payload has an ID but it doesn't match the envelope
    pub fn from_record<T>(envelope: OutgoingEnvelope, record: T) -> error::Result<Self>
    where
        T: Serialize,
    {
        let mut payload = serde_json::to_value(record)?;
        if let Some(ref mut map) = payload.as_object_mut() {
            if let Some(id) = map.remove("id").as_ref().and_then(|v| v.as_str()) {
                assert_eq!(id, envelope.id);
                assert!(
                    envelope.id.is_valid_for_sync_server(),
                    "record's ID is invalid"
                );
            }
        };
        Ok(Self {
            envelope,
            cleartext: serde_json::to_string(&payload)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Default, Debug, PartialEq, Serialize, Deserialize)]
    struct TestStruct {
        id: Guid,
        data: u32,
    }
    #[test]
    fn test_bridge_deser() {
        env_logger::try_init().ok();
        let json = json!({
            "id": "test",
            "cleartext": json!({"data": 1}).to_string(),
        });
        let incoming: IncomingBridgeRecord = serde_json::from_value(json).unwrap();
        assert_eq!(incoming.envelope.id, "test");
        let record = incoming.payload::<TestStruct>().record().unwrap();
        let expected = TestStruct {
            id: Guid::new("test"),
            data: 1,
        };
        assert_eq!(record, expected);
    }

    #[test]
    fn test_bridge_deser_empty_id() {
        env_logger::try_init().ok();
        let json = json!({
            "id": "",
            "cleartext": json!({"data": 1}).to_string(),
        });
        let incoming: IncomingBridgeRecord = serde_json::from_value(json).unwrap();
        // The envelope has an invalid ID, but it's not handled until we try and deserialize
        // it into a T
        assert_eq!(incoming.envelope.id, "");
        let deser_payload = incoming.payload::<TestStruct>();
        assert!(matches!(deser_payload, IncomingDeserPayload::Malformed));
    }

    #[test]
    fn test_bridge_deser_invalid() {
        env_logger::try_init().ok();
        // And a non-empty but still invalid guid.
        let json = json!({
            "id": "X".repeat(65),
            "cleartext": json!({"data": 1}).to_string(),
        });
        let incoming: IncomingBridgeRecord = serde_json::from_value(json).unwrap();
        let deser_payload = incoming.payload::<TestStruct>();
        assert!(matches!(deser_payload, IncomingDeserPayload::Malformed));
    }

    #[test]
    fn test_bridge_ser_with_id() {
        env_logger::try_init().ok();
        // When serializing, expect the ID to be in the top-level payload (ie,
        // in the envelope) but should not appear in the `cleartext` part of
        // the payload.
        let val = TestStruct {
            id: Guid::new("test"),
            data: 1,
        };
        let outgoing = OutgoingBridgeRecord::from_record_with_id(val).unwrap();

        // The envelope should have our ID.
        assert_eq!(outgoing.envelope.id, Guid::new("test"));

        // and make sure `cleartext` part of the payload only has data.
        let ct_payload = serde_json::from_str::<serde_json::Value>(&outgoing.cleartext).unwrap();
        let ct_map = ct_payload.as_object().unwrap();
        assert_eq!(ct_map.len(), 1);
        assert_eq!(ct_map.get("id"), None);
        assert_eq!(ct_map.get("data").unwrap().as_u64().unwrap(), 1);
    }

    #[test]
    fn test_bridge_ser_with_envelope() {
        env_logger::try_init().ok();
        // When serializing, expect the ID to be in the top-level payload (ie,
        // in the envelope) but should not appear in the `cleartext` part of
        // the payload.
        let val = TestStruct {
            id: Guid::new("test"),
            data: 1,
        };
        let envelope: OutgoingEnvelope = Guid::new("test").into();
        let outgoing = OutgoingBridgeRecord::from_record(envelope, val).unwrap();

        // The envelope should have our ID.
        assert_eq!(outgoing.envelope.id, Guid::new("test"));

        // and make sure `cleartext` part of the payload only has data.
        let ct_payload = serde_json::from_str::<serde_json::Value>(&outgoing.cleartext).unwrap();
        let ct_map = ct_payload.as_object().unwrap();
        assert_eq!(ct_map.len(), 1);
        assert_eq!(ct_map.get("id"), None);
        assert_eq!(ct_map.get("data").unwrap().as_u64().unwrap(), 1);
    }

    #[test]
    #[should_panic]
    fn test_bridge_ser_no_ids() {
        env_logger::try_init().ok();
        #[derive(Serialize)]
        struct StructWithNoId {
            data: u32,
        }
        let val = StructWithNoId { data: 1 };
        let _ = OutgoingBridgeRecord::from_record_with_id(val);
    }

    #[test]
    #[should_panic]
    fn test_bridge_ser_not_object() {
        env_logger::try_init().ok();
        let _ = OutgoingBridgeRecord::from_record_with_id(json!("string"));
    }

    #[test]
    #[should_panic]
    fn test_bridge_ser_mismatched_ids() {
        env_logger::try_init().ok();
        let val = TestStruct {
            id: Guid::new("test"),
            data: 1,
        };
        let envelope: OutgoingEnvelope = Guid::new("different").into();
        let _ = OutgoingBridgeRecord::from_record(envelope, val);
    }

    #[test]
    #[should_panic]
    fn test_bridge_empty_id() {
        env_logger::try_init().ok();
        let val = TestStruct {
            id: Guid::new(""),
            data: 1,
        };
        let _ = OutgoingBridgeRecord::from_record_with_id(val);
    }

    #[test]
    #[should_panic]
    fn test_bridge_invalid_id() {
        env_logger::try_init().ok();
        let val = TestStruct {
            id: Guid::new(&"X".repeat(65)),
            data: 1,
        };
        let _ = OutgoingBridgeRecord::from_record_with_id(val);
    }
}

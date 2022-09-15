/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error;
use crate::key_bundle::KeyBundle;
use crate::Payload;
use crate::ServerTimestamp;
use lazy_static::lazy_static;
use serde::de::{Deserialize, DeserializeOwned};
use serde::ser::Serialize;
use serde_derive::*;
use std::ops::{Deref, DerefMut};
use sync_guid::Guid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BsoRecord<T> {
    pub id: Guid,

    // It's not clear to me if this actually can be empty in practice.
    // firefox-ios seems to think it can...
    #[serde(default = "String::new")]
    pub collection: String,

    #[serde(skip_serializing)]
    // If we don't give it a default, we fail to deserialize
    // items we wrote out during tests and such.
    #[serde(default = "ServerTimestamp::default")]
    pub modified: ServerTimestamp,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sortindex: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u32>,

    // We do some serde magic here with serde to parse the payload from JSON as we deserialize.
    // This avoids having a separate intermediate type that only exists so that we can deserialize
    // it's payload field as JSON (Especially since this one is going to exist more-or-less just so
    // that we can decrypt the data...)
    #[serde(
        with = "as_json",
        bound(serialize = "T: Serialize", deserialize = "T: DeserializeOwned")
    )]
    pub payload: T,
}

impl<T> BsoRecord<T> {
    #[inline]
    pub fn map_payload<P, F>(self, mapper: F) -> BsoRecord<P>
    where
        F: FnOnce(T) -> P,
    {
        BsoRecord {
            id: self.id,
            collection: self.collection,
            modified: self.modified,
            sortindex: self.sortindex,
            ttl: self.ttl,
            payload: mapper(self.payload),
        }
    }

    #[inline]
    pub fn with_payload<P>(self, payload: P) -> BsoRecord<P> {
        self.map_payload(|_| payload)
    }

    #[inline]
    pub fn new_record(id: String, coll: String, payload: T) -> BsoRecord<T> {
        BsoRecord {
            id: id.into(),
            collection: coll,
            ttl: None,
            sortindex: None,
            modified: ServerTimestamp::default(),
            payload,
        }
    }

    pub fn try_map_payload<P, E>(
        self,
        mapper: impl FnOnce(T) -> Result<P, E>,
    ) -> Result<BsoRecord<P>, E> {
        self.map_payload(mapper).transpose()
    }

    pub fn map_payload_or<P>(self, mapper: impl FnOnce(T) -> Option<P>) -> Option<BsoRecord<P>> {
        self.map_payload(mapper).transpose()
    }

    #[inline]
    pub fn into_timestamped_payload(self) -> (T, ServerTimestamp) {
        (self.payload, self.modified)
    }
}

impl<T> BsoRecord<Option<T>> {
    /// Helper to improve ergonomics for handling records that might be tombstones.
    #[inline]
    pub fn transpose(self) -> Option<BsoRecord<T>> {
        let BsoRecord {
            id,
            collection,
            modified,
            sortindex,
            ttl,
            payload,
        } = self;
        payload.map(|p| BsoRecord {
            id,
            collection,
            modified,
            sortindex,
            ttl,
            payload: p,
        })
    }
}

impl<T, E> BsoRecord<Result<T, E>> {
    #[inline]
    pub fn transpose(self) -> Result<BsoRecord<T>, E> {
        let BsoRecord {
            id,
            collection,
            modified,
            sortindex,
            ttl,
            payload,
        } = self;
        match payload {
            Ok(p) => Ok(BsoRecord {
                id,
                collection,
                modified,
                sortindex,
                ttl,
                payload: p,
            }),
            Err(e) => Err(e),
        }
    }
}

impl<T> Deref for BsoRecord<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        &self.payload
    }
}

impl<T> DerefMut for BsoRecord<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.payload
    }
}

impl CleartextBso {
    pub fn from_payload(mut payload: Payload, collection: impl Into<String>) -> Self {
        let id = payload.id.clone();
        let sortindex: Option<i32> = payload.take_auto_field("sortindex");
        let ttl: Option<u32> = payload.take_auto_field("ttl");
        BsoRecord {
            id,
            collection: collection.into(),
            modified: ServerTimestamp::default(), // Doesn't matter.
            sortindex,
            ttl,
            payload,
        }
    }
}

pub type EncryptedBso = BsoRecord<EncryptedPayload>;
pub type CleartextBso = BsoRecord<Payload>;

// Contains the methods to automatically deserialize the payload to/from json.
mod as_json {
    use serde::de::{self, Deserialize, DeserializeOwned, Deserializer};
    use serde::ser::{self, Serialize, Serializer};

    pub fn serialize<T, S>(t: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        let j = serde_json::to_string(t).map_err(ser::Error::custom)?;
        serializer.serialize_str(&j)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: DeserializeOwned,
        D: Deserializer<'de>,
    {
        let j = String::deserialize(deserializer)?;
        serde_json::from_str(&j).map_err(de::Error::custom)
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct EncryptedPayload {
    #[serde(rename = "IV")]
    pub iv: String,
    pub hmac: String,
    pub ciphertext: String,
}

// This is a little cludgey but I couldn't think of another way to have easy deserialization
// without a bunch of wrapper types, while still only serializing a single time in the
// postqueue.
lazy_static! {
    // The number of bytes taken up by padding in a EncryptedPayload.
    static ref EMPTY_ENCRYPTED_PAYLOAD_SIZE: usize = serde_json::to_string(
        &EncryptedPayload { iv: "".into(), hmac: "".into(), ciphertext: "".into() }
    ).unwrap().len();
}

impl EncryptedPayload {
    #[inline]
    pub fn serialized_len(&self) -> usize {
        (*EMPTY_ENCRYPTED_PAYLOAD_SIZE) + self.ciphertext.len() + self.hmac.len() + self.iv.len()
    }

    pub fn decrypt_and_parse_payload<T>(&self, key: &KeyBundle) -> error::Result<T>
    where
        for<'a> T: Deserialize<'a>,
    {
        let cleartext = key.decrypt(&self.ciphertext, &self.iv, &self.hmac)?;
        Ok(serde_json::from_str(&cleartext)?)
    }

    pub fn from_cleartext_payload<T: Serialize>(
        key: &KeyBundle,
        cleartext_payload: &T,
    ) -> error::Result<Self> {
        let cleartext = serde_json::to_string(cleartext_payload)?;
        let (enc_base64, iv_base64, hmac_base16) =
            key.encrypt_bytes_rand_iv(cleartext.as_bytes())?;
        Ok(EncryptedPayload {
            iv: iv_base64,
            hmac: hmac_base16,
            ciphertext: enc_base64,
        })
    }
}

impl EncryptedBso {
    pub fn decrypt(self, key: &KeyBundle) -> error::Result<CleartextBso> {
        let new_payload = self
            .payload
            .decrypt_and_parse_payload::<Payload>(key)?
            .with_auto_field("sortindex", self.sortindex)
            .with_auto_field("ttl", self.ttl);

        let result = self.with_payload(new_payload);
        Ok(result)
    }

    pub fn decrypt_as<T>(self, key: &KeyBundle) -> error::Result<BsoRecord<T>>
    where
        for<'a> T: Deserialize<'a>,
    {
        self.decrypt(key)?.into_record::<T>()
    }
}

impl CleartextBso {
    pub fn encrypt(self, key: &KeyBundle) -> error::Result<EncryptedBso> {
        let encrypted_payload = EncryptedPayload::from_cleartext_payload(key, &self.payload)?;
        Ok(self.with_payload(encrypted_payload))
    }

    pub fn into_record<T>(self) -> error::Result<BsoRecord<T>>
    where
        for<'a> T: Deserialize<'a>,
    {
        Ok(self.try_map_payload(Payload::into_record)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use serde_json::Value as JsonValue;

    #[test]
    fn test_deserialize_enc() {
        let serialized = r#"{
            "id": "1234",
            "collection": "passwords",
            "modified": 12344321.0,
            "payload": "{\"IV\": \"aaaaa\", \"hmac\": \"bbbbb\", \"ciphertext\": \"ccccc\"}"
        }"#;
        let record: BsoRecord<EncryptedPayload> = serde_json::from_str(serialized).unwrap();
        assert_eq!(&record.id, "1234");
        assert_eq!(&record.collection, "passwords");
        assert_eq!((record.modified.0 - 12_344_321_000).abs(), 0);
        assert_eq!(record.sortindex, None);
        assert_eq!(&record.payload.iv, "aaaaa");
        assert_eq!(&record.payload.hmac, "bbbbb");
        assert_eq!(&record.payload.ciphertext, "ccccc");
    }

    #[test]
    fn test_deserialize_autofields() {
        let serialized = r#"{
            "id": "1234",
            "collection": "passwords",
            "modified": 12344321.0,
            "sortindex": 100,
            "ttl": 99,
            "payload": "{\"IV\": \"aaaaa\", \"hmac\": \"bbbbb\", \"ciphertext\": \"ccccc\"}"
        }"#;
        let record: BsoRecord<EncryptedPayload> = serde_json::from_str(serialized).unwrap();
        assert_eq!(record.sortindex, Some(100));
        assert_eq!(record.ttl, Some(99));
    }

    #[test]
    fn test_serialize_enc() {
        let goal = r#"{"id":"1234","collection":"passwords","payload":"{\"IV\":\"aaaaa\",\"hmac\":\"bbbbb\",\"ciphertext\":\"ccccc\"}"}"#;
        let record = BsoRecord {
            id: "1234".into(),
            modified: ServerTimestamp(999), // shouldn't be serialized by client no matter what it's value is
            collection: "passwords".into(),
            sortindex: None,
            ttl: None,
            payload: EncryptedPayload {
                iv: "aaaaa".into(),
                hmac: "bbbbb".into(),
                ciphertext: "ccccc".into(),
            },
        };
        let actual = serde_json::to_string(&record).unwrap();
        assert_eq!(actual, goal);

        let val_str_payload: serde_json::Value = serde_json::from_str(goal).unwrap();
        assert_eq!(
            val_str_payload["payload"].as_str().unwrap().len(),
            record.payload.serialized_len()
        )
    }

    #[test]
    fn test_roundtrip_crypt_tombstone() {
        let orig_record = CleartextBso::from_payload(
            Payload::from_json(json!({ "id": "aaaaaaaaaaaa", "deleted": true, })).unwrap(),
            "dummy",
        );

        assert!(orig_record.is_tombstone());

        let keybundle = KeyBundle::new_random().unwrap();

        let encrypted = orig_record.clone().encrypt(&keybundle).unwrap();

        // While we're here, check on EncryptedPayload::serialized_len
        let val_rec =
            serde_json::from_str::<JsonValue>(&serde_json::to_string(&encrypted).unwrap()).unwrap();

        assert_eq!(
            encrypted.payload.serialized_len(),
            val_rec["payload"].as_str().unwrap().len()
        );

        let decrypted: CleartextBso = encrypted.decrypt(&keybundle).unwrap();
        assert!(decrypted.is_tombstone());
        assert_eq!(decrypted, orig_record);
    }

    #[test]
    fn test_roundtrip_crypt_record() {
        let payload = json!({ "id": "aaaaaaaaaaaa", "age": 105, "meta": "data" });
        let orig_record =
            CleartextBso::from_payload(Payload::from_json(payload.clone()).unwrap(), "dummy");

        assert!(!orig_record.is_tombstone());

        let keybundle = KeyBundle::new_random().unwrap();

        let encrypted = orig_record.clone().encrypt(&keybundle).unwrap();

        // While we're here, check on EncryptedPayload::serialized_len
        let val_rec =
            serde_json::from_str::<JsonValue>(&serde_json::to_string(&encrypted).unwrap()).unwrap();
        assert_eq!(
            encrypted.payload.serialized_len(),
            val_rec["payload"].as_str().unwrap().len()
        );

        let decrypted = encrypted.decrypt(&keybundle).unwrap();
        assert!(!decrypted.is_tombstone());
        assert_eq!(decrypted, orig_record);
        assert_eq!(serde_json::to_value(decrypted.payload).unwrap(), payload);
    }

    #[test]
    fn test_record_auto_fields() {
        let payload = json!({ "id": "aaaaaaaaaaaa", "age": 105, "meta": "data", "sortindex": 100, "ttl": 99 });
        let bso = CleartextBso::from_payload(Payload::from_json(payload).unwrap(), "dummy");

        // We don't want the keys ending up in the actual record data on the server.
        assert!(!bso.payload.data.contains_key("sortindex"));
        assert!(!bso.payload.data.contains_key("ttl"));

        // But we do want them in the BsoRecord.
        assert_eq!(bso.sortindex, Some(100));
        assert_eq!(bso.ttl, Some(99));

        let keybundle = KeyBundle::new_random().unwrap();
        let encrypted = bso.encrypt(&keybundle).unwrap();

        let decrypted = encrypted.decrypt(&keybundle).unwrap();
        // We add auto fields during decryption.
        assert_eq!(decrypted.payload.data["sortindex"], 100);
        assert_eq!(decrypted.payload.data["ttl"], 99);

        assert_eq!(decrypted.sortindex, Some(100));
        assert_eq!(decrypted.ttl, Some(99));
    }
    #[test]
    fn test_record_bad_hmac() {
        let payload = json!({ "id": "aaaaaaaaaaaa", "age": 105, "meta": "data", "sortindex": 100, "ttl": 99 });
        let bso = CleartextBso::from_payload(Payload::from_json(payload).unwrap(), "dummy");

        let keybundle = KeyBundle::new_random().unwrap();
        let encrypted = bso.encrypt(&keybundle).unwrap();
        let keybundle2 = KeyBundle::new_random().unwrap();

        let e = encrypted
            .decrypt(&keybundle2)
            .expect_err("Should fail because wrong keybundle");

        // Note: Error isn't PartialEq, so.
        match e {
            error::Error::CryptoError(_) => {
                // yay.
            }
            other => {
                panic!("Expected Crypto Error, got {:?}", other);
            }
        }
    }
}

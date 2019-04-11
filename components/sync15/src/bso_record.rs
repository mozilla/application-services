/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error;
use crate::key_bundle::KeyBundle;
use crate::util::ServerTimestamp;
use lazy_static::lazy_static;
use serde::de::{Deserialize, DeserializeOwned};
use serde::ser::Serialize;
use serde_derive::*;
use serde_json::{self, Map, Value as JsonValue};
use std::convert::From;
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BsoRecord<T> {
    pub id: String,

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
            id,
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
        match payload {
            Some(p) => Some(BsoRecord {
                id,
                collection,
                modified,
                sortindex,
                ttl,
                payload: p,
            }),
            None => None,
        }
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

/// Represents the decrypted payload in a Bso. Provides a minimal layer of type safety to avoid double-encrypting.
///
/// Note: If we implement a full sync client in rust we may want to consider using stronger types for each record
/// (we did this in the past as well), but for now, since everything is just going over the FFI, there's not a lot of
/// benefit here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Payload {
    pub id: String,

    #[serde(default)]
    #[serde(skip_serializing_if = "is_false")]
    pub deleted: bool,

    #[serde(flatten)]
    pub data: Map<String, JsonValue>,
}

// TODO: Move skip_if_default from places to something shared
#[inline]
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

impl Payload {
    #[inline]
    pub fn new_tombstone(id: String) -> Payload {
        Payload {
            id,
            deleted: true,
            data: Map::new(),
        }
    }

    #[inline]
    pub fn new_tombstone_with_ttl(id: String, ttl: u32) -> Payload {
        let mut result = Payload::new_tombstone(id);
        result.data.insert("ttl".into(), ttl.into());
        result
    }

    #[inline]
    pub fn with_sortindex(mut self, index: i32) -> Payload {
        self.data.insert("sortindex".into(), index.into());
        self
    }

    #[inline]
    pub fn id(&self) -> &str {
        &self.id[..]
    }

    #[inline]
    pub fn is_tombstone(&self) -> bool {
        self.deleted
    }

    pub fn into_bso(mut self, collection: String) -> CleartextBso {
        let id = self.id.clone();
        let sortindex: Option<i32> = self.take_auto_field("sortindex");
        let ttl: Option<u32> = self.take_auto_field("ttl");
        CleartextBso {
            id,
            collection,
            modified: 0.0.into(), // Doesn't matter.
            sortindex,
            ttl,
            payload: self,
        }
    }

    pub fn from_json(value: JsonValue) -> error::Result<Payload> {
        Ok(serde_json::from_value(value)?)
    }

    pub fn into_record<T>(self) -> error::Result<T>
    where
        for<'a> T: Deserialize<'a>,
    {
        Ok(serde_json::from_value(JsonValue::from(self))?)
    }

    pub fn from_record<T: Serialize>(v: T) -> error::Result<Payload> {
        // TODO: This is dumb, we do to_value and then from_value. If we end up using this
        // method a lot we should rethink... As it is it should just be for uploading
        // meta/global or crypto/keys which is rare enough that it doesn't matter.
        Ok(Payload::from_json(serde_json::to_value(v)?)?)
    }

    pub fn into_json_string(self) -> String {
        serde_json::to_string(&JsonValue::from(self))
            .expect("JSON.stringify failed, which shouldn't be possible")
    }

    /// "Auto" fields are fields like 'sortindex' (and potentially 'ttl' in
    /// the future) which are:
    ///
    /// - Added to the payload automatically when deserializing if present on
    ///   the incoming BSO.
    /// - Removed from the payload automatically and attached to the BSO if
    ///   present on the outgoing payload.
    fn add_auto_field<T: Into<JsonValue>>(&mut self, name: &str, v: Option<T>) {
        // This is a little dubious, but it seems like if we have a e.g. `sortindex` field on the payload
        // it's going to be a bug if we use it instead of the "real" sort index.
        if self.data.contains_key(name) {
            log::warn!(
                "Payload for record {} already contains 'automatic' field \"{}\"? \
                 Overwriting with 'real' value",
                self.id,
                name
            );
        }

        if let Some(value) = v {
            self.data.insert(name.into(), value.into());
        } else {
            self.data.remove(name);
        }
    }

    fn take_auto_field<V>(&mut self, name: &str) -> Option<V>
    where
        for<'a> V: Deserialize<'a>,
    {
        let v = self.data.remove(name)?;
        match serde_json::from_value(v) {
            Ok(v) => Some(v),
            Err(e) => {
                log::error!(
                    "Automatic field {} exists on payload, but cannot be deserialized: {}",
                    name,
                    e
                );
                None
            }
        }
    }
}

impl From<Payload> for JsonValue {
    fn from(cleartext: Payload) -> Self {
        let Payload {
            mut data,
            id,
            deleted,
        } = cleartext;
        data.insert("id".to_string(), JsonValue::String(id));
        if deleted {
            data.insert("deleted".to_string(), JsonValue::Bool(true));
        }
        JsonValue::Object(data)
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
        if !key.verify_hmac_string(&self.hmac, &self.ciphertext)? {
            return Err(error::ErrorKind::HmacMismatch.into());
        }

        let iv = base64::decode(&self.iv)?;
        let ciphertext = base64::decode(&self.ciphertext)?;
        let cleartext = key.decrypt(&ciphertext, &iv)?;

        Ok(serde_json::from_str(&cleartext)?)
    }

    pub fn from_cleartext_payload<T: Serialize>(
        key: &KeyBundle,
        cleartext_payload: &T,
    ) -> error::Result<Self> {
        let cleartext = serde_json::to_string(cleartext_payload)?;
        let (enc_bytes, iv) = key.encrypt_bytes_rand_iv(&cleartext.as_bytes())?;
        let iv_base64 = base64::encode(&iv);
        let enc_base64 = base64::encode(&enc_bytes);
        let hmac = key.hmac_string(enc_base64.as_bytes())?;
        Ok(EncryptedPayload {
            iv: iv_base64,
            hmac,
            ciphertext: enc_base64,
        })
    }
}

impl EncryptedBso {
    pub fn decrypt(self, key: &KeyBundle) -> error::Result<CleartextBso> {
        let mut new_payload: Payload = self.payload.decrypt_and_parse_payload(key)?;
        // This is a slightly dodgy place to do this, but whatever.
        new_payload.add_auto_field("sortindex", self.sortindex);
        new_payload.add_auto_field("ttl", self.ttl);

        let result = self.with_payload(new_payload);
        Ok(result)
    }

    pub fn decrypt_as<T>(self, key: &KeyBundle) -> error::Result<BsoRecord<T>>
    where
        for<'a> T: Deserialize<'a>,
    {
        Ok(self.decrypt(key)?.into_record::<T>()?)
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
        assert!((record.modified.0 - 1234_4321.0).abs() < std::f64::EPSILON);
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
            modified: ServerTimestamp(999.0), // shouldn't be serialized by client no matter what it's value is
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
        let orig_record = Payload::from_json(json!({ "id": "aaaaaaaaaaaa", "deleted": true, }))
            .unwrap()
            .into_bso("dummy".into());
        assert!(orig_record.is_tombstone());

        let keybundle = KeyBundle::new_random().unwrap();

        let encrypted = orig_record.clone().encrypt(&keybundle).unwrap();

        assert!(keybundle
            .verify_hmac_string(&encrypted.payload.hmac, &encrypted.payload.ciphertext)
            .unwrap());

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
        let orig_record = Payload::from_json(payload.clone())
            .unwrap()
            .into_bso("dummy".into());

        assert!(!orig_record.is_tombstone());

        let keybundle = KeyBundle::new_random().unwrap();

        let encrypted = orig_record.clone().encrypt(&keybundle).unwrap();

        assert!(keybundle
            .verify_hmac_string(&encrypted.payload.hmac, &encrypted.payload.ciphertext)
            .unwrap());

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
        let bso = Payload::from_json(payload.clone())
            .unwrap()
            .into_bso("dummy".into());

        // We don't want the keys ending up in the actual record data on the server.
        assert!(!bso.payload.data.contains_key("sortindex"));
        assert!(!bso.payload.data.contains_key("ttl"));

        // But we do want them in the BsoRecord.
        assert_eq!(bso.sortindex, Some(100));
        assert_eq!(bso.ttl, Some(99));

        let keybundle = KeyBundle::new_random().unwrap();
        let encrypted = bso.clone().encrypt(&keybundle).unwrap();

        assert!(keybundle
            .verify_hmac_string(&encrypted.payload.hmac, &encrypted.payload.ciphertext)
            .unwrap());

        let decrypted = encrypted.decrypt(&keybundle).unwrap();
        // We add auto fields during decryption.
        assert_eq!(decrypted.payload.data["sortindex"], 100);
        assert_eq!(decrypted.payload.data["ttl"], 99);

        assert_eq!(decrypted.sortindex, Some(100));
        assert_eq!(decrypted.ttl, Some(99));
    }
}

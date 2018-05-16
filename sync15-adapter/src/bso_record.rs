/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde::de::{Deserialize, DeserializeOwned};
use serde::ser::Serialize;
use serde_json::{self, Value as JsonValue, Map};
use error;
use base64;
use std::ops::{Deref, DerefMut};
use std::convert::From;
use key_bundle::KeyBundle;
use util::ServerTimestamp;

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
    #[serde(with = "as_json", bound(
        serialize = "T: Serialize",
        deserialize = "T: DeserializeOwned"))]
    pub payload: T,
}

impl<T> BsoRecord<T> {
    #[inline]
    pub fn map_payload<P, F>(self, mapper: F) -> BsoRecord<P> where F: FnOnce(T) -> P {
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
    pub fn new_non_record(id: String, coll: String, payload: T) -> BsoRecord<T> {
        BsoRecord {
            id: id.into(),
            collection: coll.into(),
            ttl: None,
            sortindex: None,
            modified: ServerTimestamp::default(),
            payload,
        }
    }

    pub fn try_map_payload<P, E>(
        self,
        mapper: impl FnOnce(T) -> Result<P, E>
    ) -> Result<BsoRecord<P>, E> {
        self.map_payload(mapper).transpose()
    }

    pub fn map_payload_or<P>(
        self,
        mapper: impl FnOnce(T) -> Option<P>
    ) -> Option<BsoRecord<P>> {
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
        let BsoRecord { id, collection, modified, sortindex, ttl, payload } = self;
        match payload {
            Some(p) => Some(BsoRecord { id, collection, modified, sortindex, ttl, payload: p }),
            None => None
        }
    }
}

impl<T, E> BsoRecord<Result<T, E>> {
    #[inline]
    pub fn transpose(self) -> Result<BsoRecord<T>, E> {
        let BsoRecord { id, collection, modified, sortindex, ttl, payload } = self;
        match payload {
            Ok(p) => Ok(BsoRecord { id, collection, modified, sortindex, ttl, payload: p }),
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
pub struct Cleartext {
    pub id: String,

    #[serde(default)]
    #[serde(skip_serializing_if = "is_false")]
    pub deleted: bool,

    #[serde(flatten)]
    pub data: Map<String, JsonValue>,
}

// `#[serde(skip_if)]` only allows a function (not an expression).
// Is there a builtin way to do this?
#[inline]
fn is_false(b: &bool) -> bool {
    !*b
}

impl Cleartext {

    #[inline]
    pub fn new_tombstone(id: String) -> Cleartext {
        Cleartext { id, deleted: true, data: Map::new() }
    }

    #[inline]
    pub fn id(&self) -> &str {
        &self.id[..]
    }

    #[inline]
    pub fn is_tombstone(&self) -> bool {
        self.deleted
    }

    pub fn into_bso(
        self,
        collection: String
    ) -> CleartextBso {
        let id = self.id.clone();
        CleartextBso {
            id,
            collection,
            modified: 0.0.into(), // Doesn't matter.
            sortindex: None, // Should we let consumer's set this?
            ttl: None, // Should we let consumer's set this?
            payload: self,
        }
    }

    pub fn from_json(value: JsonValue) -> error::Result<Cleartext> {
        Ok(serde_json::from_value(value)?)
    }

    pub fn into_record<T>(self) -> error::Result<T> where for<'a> T: Deserialize<'a> {
        Ok(serde_json::from_value(JsonValue::from(self))?)
    }

    pub fn from_record<T: Serialize>(v: T) -> error::Result<Cleartext> {
        // TODO: This is dumb, we do to_value and then from_value. If we end up using this
        // method a lot we should rethink... As it is it should just be for uploading
        // meta/global or crypto/keys which is rare enough that it doesn't matter.
        Ok(Cleartext::from_json(serde_json::to_value(v)?)?)
    }

    pub fn into_json_string(self) -> String {
        serde_json::to_string(&JsonValue::from(self))
            .expect("JSON.stringify failed, wish shouldn't be possible")
    }

}

impl From<Cleartext> for JsonValue {
    fn from(cleartext: Cleartext) -> Self {
        let Cleartext { mut data, id, deleted } = cleartext;
        data.insert("id".to_string(), JsonValue::String(id));
        if deleted {
            data.insert("deleted".to_string(), JsonValue::Bool(true));
        }
        JsonValue::Object(data)
    }
}

pub type EncryptedBso = BsoRecord<EncryptedPayload>;
pub type CleartextBso = BsoRecord<Cleartext>;

// Contains the methods to automatically deserialize the payload to/from json.
mod as_json {
    use serde_json;
    use serde::de::{self, Deserialize, DeserializeOwned, Deserializer};
    use serde::ser::{self, Serialize, Serializer};

    pub fn serialize<T, S>(t: &T, serializer: S) -> Result<S::Ok, S::Error>
            where T: Serialize, S: Serializer {
        let j = serde_json::to_string(t).map_err(ser::Error::custom)?;
        serializer.serialize_str(&j)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
            where T: DeserializeOwned, D: Deserializer<'de> {
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
}

impl EncryptedBso {
    pub fn decrypt(self, key: &KeyBundle) -> error::Result<CleartextBso> {
        if !key.verify_hmac_string(&self.payload.hmac, &self.payload.ciphertext)? {
            return Err(error::ErrorKind::HmacMismatch.into());
        }

        let iv = base64::decode(&self.payload.iv)?;
        let ciphertext = base64::decode(&self.payload.ciphertext)?;
        let cleartext = key.decrypt(&ciphertext, &iv)?;

        let new_payload = serde_json::from_str(&cleartext)?;

        let result = self.with_payload(new_payload);
        Ok(result)
    }

    pub fn decrypt_as<T>(self, key: &KeyBundle) -> error::Result<BsoRecord<T>>
        where for<'a> T: Deserialize<'a>
    {
        Ok(self.decrypt(key)?.into_record::<T>()?)
    }
}

impl CleartextBso {
    pub fn encrypt(self, key: &KeyBundle) -> error::Result<EncryptedBso> {
        let cleartext = serde_json::to_string(&self.payload)?;
        let (enc_bytes, iv) = key.encrypt_bytes_rand_iv(&cleartext.as_bytes())?;
        let iv_base64 = base64::encode(&iv);
        let enc_base64 = base64::encode(&enc_bytes);
        let hmac = key.hmac_string(enc_base64.as_bytes())?;
        let result = self.with_payload(EncryptedPayload {
            iv: iv_base64,
            hmac,
            ciphertext: enc_base64,
        });
        Ok(result)
    }

    pub fn into_record<T>(self) -> error::Result<BsoRecord<T>> where for<'a> T: Deserialize<'a> {
        Ok(self.try_map_payload(|payload| payload.into_record())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(record.modified.0, 12344321.0);
        assert_eq!(&record.payload.iv, "aaaaa");
        assert_eq!(&record.payload.hmac, "bbbbb");
        assert_eq!(&record.payload.ciphertext, "ccccc");
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
            }
        };
        let actual = serde_json::to_string(&record).unwrap();
        assert_eq!(actual, goal);

        let val_str_payload: serde_json::Value = serde_json::from_str(goal).unwrap();
        assert_eq!(val_str_payload["payload"].as_str().unwrap().len(),
                   record.payload.serialized_len())
    }

    #[test]
    fn test_roundtrip_crypt_tombstone() {
        let orig_record = Cleartext::from_json(
            json!({ "id": "aaaaaaaaaaaa", "deleted": true, })
        ).unwrap().into_bso("dummy".into());
        assert!(orig_record.is_tombstone());

        let keybundle = KeyBundle::new_random().unwrap();

        let encrypted = orig_record.clone().encrypt(&keybundle).unwrap();

        assert!(keybundle.verify_hmac_string(
            &encrypted.payload.hmac,
            &encrypted.payload.ciphertext).unwrap());

        // While we're here, check on EncryptedPayload::serialized_len
        let val_rec = serde_json::from_str::<JsonValue>(
            &serde_json::to_string(&encrypted).unwrap()).unwrap();

        assert_eq!(encrypted.payload.serialized_len(),
                   val_rec["payload"].as_str().unwrap().len());

        let decrypted: CleartextBso = encrypted.decrypt(&keybundle).unwrap();
        assert!(decrypted.is_tombstone());
        assert_eq!(decrypted, orig_record);
    }

    #[test]
    fn test_roundtrip_crypt_record() {
        let payload = json!({ "id": "aaaaaaaaaaaa", "age": 105, "meta": "data" });
        let orig_record = Cleartext::from_json(payload.clone()).unwrap()
            .into_bso("dummy".into());

        assert!(!orig_record.is_tombstone());

        let keybundle = KeyBundle::new_random().unwrap();

        let encrypted = orig_record.clone().encrypt(&keybundle).unwrap();

        assert!(keybundle.verify_hmac_string(
            &encrypted.payload.hmac,
            &encrypted.payload.ciphertext
        ).unwrap());

        // While we're here, check on EncryptedPayload::serialized_len
        let val_rec = serde_json::from_str::<JsonValue>(
            &serde_json::to_string(&encrypted).unwrap()).unwrap();
        assert_eq!(encrypted.payload.serialized_len(),
                   val_rec["payload"].as_str().unwrap().len());

        let decrypted = encrypted.decrypt(&keybundle).unwrap();
        assert!(!decrypted.is_tombstone());
        assert_eq!(decrypted, orig_record);
        assert_eq!(serde_json::to_value(decrypted.payload).unwrap(), payload);
    }


}

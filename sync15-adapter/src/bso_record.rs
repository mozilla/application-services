/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use serde_json;
use error;
use base64;
use key_bundle::KeyBundle;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BsoRecord<T> {
    pub id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,

    #[serde(skip_serializing)]
    pub modified: f64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sortindex: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u32>,

    // We do some serde magic here with serde to parse the payload from JSON as we deserialize.
    // This avoids having a separate intermediate type that only exists so that we can deserialize
    // it's payload field as JSON (Especially since this one is going to exist more-or-less just so
    // that we can decrypt the data...
    #[serde(with = "as_json", bound(
        serialize = "T: Serialize",
        deserialize = "T: DeserializeOwned"))]
    pub payload: T,
}

impl<T> BsoRecord<T> {
    #[inline]
    pub fn with_payload<P>(self, payload: P) -> BsoRecord<P> {
        BsoRecord {
            id: self.id,
            collection: self.collection,
            modified: self.modified,
            sortindex: self.sortindex,
            ttl: self.ttl,
            payload: payload,
        }
    }
}

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

/// Marker trait that indicates that something is a sync record type. By not implementing this
/// for EncryptedPayload, we can statically prevent double-encrypting.
pub trait Sync15Record: Clone + DeserializeOwned + Serialize {}

impl Sync15Record for serde_json::Value {}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum MaybeTombstone<T> {
    Tombstone { id: String, deleted: bool },
    Record(T)
}

impl<T> MaybeTombstone<T> {

    #[inline]
    pub fn tombstone<R: Into<String>>(id: R) -> MaybeTombstone<T> {
        MaybeTombstone::Tombstone { id: id.into(), deleted: true }
    }

    #[inline]
    pub fn is_tombstone(&self) -> bool {
        match self {
            &MaybeTombstone::Record(_) => false,
            _ => true
        }
    }

    #[inline]
    pub fn unwrap(self) -> T {
        match self {
            MaybeTombstone::Record(record) => record,
            _ => panic!("called `MaybeTombstone::unwrap()` on a Tombstone!"),
        }
    }

    #[inline]
    pub fn expect(self, msg: &str) -> T {
        match self {
            MaybeTombstone::Record(record) => record,
            _ => panic!("{}", msg),
        }
    }

    #[inline]
    pub fn ok_or<E>(self, err: E) -> Result<T, E> {
        match self {
            MaybeTombstone::Record(record) => Ok(record),
            _ => Err(err)
        }
    }

    #[inline]
    pub fn record(self) -> Option<T> {
        match self {
            MaybeTombstone::Record(record) => Some(record),
            _ => None
        }
    }
}

impl<T> Sync15Record for MaybeTombstone<T> where T: Sync15Record {}

impl BsoRecord<EncryptedPayload> {
    pub fn decrypt<T>(self, key: &KeyBundle) -> error::Result<BsoRecord<MaybeTombstone<T>>>
            where T: DeserializeOwned {
        if !key.verify_hmac_string(&self.payload.hmac, &self.payload.ciphertext)? {
            return Err(error::ErrorKind::HmacMismatch.into());
        }

        let iv = base64::decode(&self.payload.iv)?;
        let ciphertext = base64::decode(&self.payload.ciphertext)?;
        let cleartext = key.decrypt(&ciphertext, &iv)?;

        let new_payload = serde_json::from_str::<MaybeTombstone<T>>(&cleartext)?;

        let result = self.with_payload(new_payload);
        Ok(result)
    }
}

impl<T> BsoRecord<T> where T: Sync15Record {
    pub fn encrypt(self, key: &KeyBundle) -> error::Result<BsoRecord<EncryptedPayload>> {
        let cleartext = serde_json::to_string(&self.payload)?;
        let (enc_bytes, iv) = key.encrypt_bytes_rand_iv(&cleartext.as_bytes())?;
        let iv_base64 = base64::encode(&iv);
        let enc_base64 = base64::encode(&enc_bytes);
        let hmac = key.hmac_string(enc_base64.as_bytes())?;
        let result = self.with_payload(EncryptedPayload {
            iv: iv_base64,
            hmac: hmac,
            ciphertext: enc_base64,
        });
        Ok(result)
    }
}

impl<T> BsoRecord<MaybeTombstone<T>> {
    #[inline]
    pub fn is_tombstone(&self) -> bool {
        self.payload.is_tombstone()
    }

    #[inline]
    pub fn with_record(self) -> Option<BsoRecord<T>> where T: Clone {
        // XXX how to avoid the clone w/o inlining with_payload
        match self.payload.clone() {
            MaybeTombstone::Tombstone { .. } => None,
            MaybeTombstone::Record(record) => Some(self.with_payload(record))
        }
    }

    #[inline]
    pub fn unwrap_record(self) -> BsoRecord<T> where T: Clone {
        // XXX how to avoid the clone without inlining with_payload
        let unwrapped_payload = self.payload.clone().unwrap();
        self.with_payload(unwrapped_payload)
    }
}

pub type MaybeTombstoneRecord<T> = BsoRecord<MaybeTombstone<T>>;

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
        assert_eq!(&record.collection.unwrap(), "passwords");
        assert_eq!(record.modified, 12344321.0);
        assert_eq!(&record.payload.iv, "aaaaa");
        assert_eq!(&record.payload.hmac, "bbbbb");
        assert_eq!(&record.payload.ciphertext, "ccccc");
    }

    #[test]
    fn test_serialize_enc() {
        let goal = r#"{"id":"1234","collection":"passwords","payload":"{\"IV\":\"aaaaa\",\"hmac\":\"bbbbb\",\"ciphertext\":\"ccccc\"}"}"#;
        let record = BsoRecord {
            id: "1234".into(),
            modified: 999.0, // shouldn't be serialized by client no matter what it's value is
            collection: Some("passwords".into()),
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
    }

    #[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
    struct DummyRecord {
        id: String,
        age: i64,
        meta: String,
    }

    impl Sync15Record for DummyRecord {}

    #[test]
    fn test_roundtrip_crypt_tombstone() {
        let orig_record: MaybeTombstoneRecord<DummyRecord> = BsoRecord {
            id: "aaaaaaaaaaaa".into(),
            collection: None,
            modified: 1234.0,
            sortindex: None,
            ttl: None,
            payload: MaybeTombstone::tombstone("aaaaaaaaaaaa")
        };

        assert!(orig_record.is_tombstone());

        let keybundle = KeyBundle::new_random().unwrap();

        let encrypted = orig_record.clone().encrypt(&keybundle).unwrap();

        assert!(keybundle.verify_hmac_string(
            &encrypted.payload.hmac, &encrypted.payload.ciphertext).unwrap());

        let decrypted: MaybeTombstoneRecord<DummyRecord> = encrypted.decrypt(&keybundle).unwrap();
        assert!(decrypted.is_tombstone());
        assert_eq!(decrypted, orig_record);
    }

    #[test]
    fn test_roundtrip_crypt_record() {
        let orig_record: MaybeTombstoneRecord<DummyRecord> = BsoRecord {
            id: "aaaaaaaaaaaa".into(),
            collection: None,
            modified: 1234.0,
            sortindex: None,
            ttl: None,
            payload: MaybeTombstone::Record(DummyRecord {
                id: "aaaaaaaaaaaa".into(),
                age: 105,
                meta: "data".into()
            })
        };

        assert!(!orig_record.is_tombstone());

        let keybundle = KeyBundle::new_random().unwrap();

        let encrypted = orig_record.clone().encrypt(&keybundle).unwrap();

        assert!(keybundle.verify_hmac_string(
            &encrypted.payload.hmac, &encrypted.payload.ciphertext).unwrap());

        let decrypted: MaybeTombstoneRecord<DummyRecord> = encrypted.decrypt(&keybundle).unwrap();
        assert!(!decrypted.is_tombstone());
        assert_eq!(decrypted, orig_record);
    }
}

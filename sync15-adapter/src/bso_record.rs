/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::ser::{self, Serialize, Serializer, SerializeStruct};
use serde_json;
use error;
use base64;
use key_bundle::KeyBundle;

// use std::collections::HashMap;
use std::marker::PhantomData;
use std::fmt;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BsoRecord where {
    pub id: String,

    pub collection: Option<String>,
    pub modified: f64,
    pub sortindex: Option<i32>,
    pub ttl: Option<u32>,

    // We do some serde magic here with serde to parse the payload from JSON as we deserialize.
    // This avoids having a separate intermediate type that only exists so that we can deserialize
    // it's payload field as JSON (Especially since this one is going to exist more-or-less just so
    // that we can decrypt the data...
    #[serde(deserialize_with = "deserialize_json")]
    pub payload: serde_json::Value,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct EncryptedPayload {
    #[serde(rename = "IV")]
    pub iv: String,
    pub hmac: String,
    pub ciphertext: String,
}

// Custom deserializer to handle auto-deserializing the payload from JSON.
fn deserialize_json<'de, T, D>(deserializer: D) -> Result<T, D::Error> where for <'a> T: Deserialize<'a>, D: Deserializer<'de> {
    struct DeserializeNestedJson<T>(PhantomData<fn() -> T>);

    impl<'de, T> Visitor<'de> for DeserializeNestedJson<T> where for<'a> T: Deserialize<'a> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("The JSON-encoded payload as string")
        }

        fn visit_str<E>(self, value: &str) -> Result<T, E> where E: de::Error {
            serde_json::from_str(&value).map_err(|e| de::Error::custom(e))
        }
    }

    let visitor = DeserializeNestedJson(PhantomData);
    deserializer.deserialize_str(visitor)
}

// Custom serializer to handle auto-serializing the payload to JSON
impl Serialize for BsoRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
        // Serialize the object we hold in our payload to a string right away.
        let payload_json = serde_json::to_string(&self.payload).map_err(|e| ser::Error::custom(e))?;

        // We always serialize id and payload, and serialize collection, ttl, and sortindex iff.
        // they are present. Annoyingly, serialize_struct requires us tell how many we'll serialize
        // up-front.
        let num_fields = 2 + (self.collection.is_some() as usize)
                           + (self.ttl.is_some() as usize)
                           + (self.sortindex.is_some() as usize);

        // Note: The name here doesn't show up in the output. At least, not for JSON.
        let mut state = serializer.serialize_struct("BsoRecord", num_fields)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("payload", &payload_json)?;

        if let &Some(ref collection) = &self.collection {
            state.serialize_field("collection", collection)?;
        }
        if let &Some(ref sortindex) = &self.sortindex {
            state.serialize_field("sortindex", sortindex)?;
        }
        if let &Some(ref ttl) = &self.ttl {
            state.serialize_field("ttl", ttl)?;
        }
        state.end()
    }
}

impl BsoRecord {
    pub fn decrypt(&mut self, key: &KeyBundle) -> error::Result<()> {
        assert!(self.is_encrypted());
        let payload_data: EncryptedPayload = serde_json::from_value(self.payload.clone())?;
        if !key.verify_hmac_string(&payload_data.hmac, &payload_data.ciphertext)? {
            return Err(error::ErrorKind::HmacMismatch.into());
        }

        let iv = base64::decode(&payload_data.iv)?;
        let ciphertext = base64::decode(&payload_data.ciphertext)?;
        let cleartext = key.decrypt(&ciphertext, &iv)?;

        self.payload = serde_json::to_value(cleartext)?;
        Ok(())
    }

    pub fn is_encrypted(&self) -> bool {
        if let Some(map) = self.payload.as_object() {
            map.contains_key("IV") && map.contains_key("hmac") && map.contains_key("ciphertext")
        } else {
            false
        }
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
        let record: BsoRecord = serde_json::from_str(serialized).unwrap();
        assert_eq!(&record.id, "1234");
        assert_eq!(&record.collection.unwrap(), "passwords");
        assert_eq!(record.modified, 12344321.0);
        let payload: EncryptedPayload = serde_json::from_value(record.payload).unwrap();
        assert_eq!(&payload.iv, "aaaaa");
        assert_eq!(&payload.hmac, "bbbbb");
        assert_eq!(&payload.ciphertext, "ccccc");
    }
    #[test]
    fn test_serialize_enc() {
        let goal = r#"{"id":"1234","payload":"{\"IV\":\"aaaaa\",\"ciphertext\":\"ccccc\",\"hmac\":\"bbbbb\"}","collection":"passwords"}"#;
        let record = BsoRecord {
            id: "1234".into(),
            modified: 999.0, // shouldn't be serialized by client no matter what it's value is
            collection: Some("passwords".into()),
            sortindex: None,
            ttl: None,
            payload: serde_json::to_value(EncryptedPayload {
                iv: "aaaaa".into(),
                hmac: "bbbbb".into(),
                ciphertext: "ccccc".into(),
            }).unwrap()
        };
        let actual = serde_json::to_string(&record).unwrap();
        assert_eq!(actual, goal);
    }
}

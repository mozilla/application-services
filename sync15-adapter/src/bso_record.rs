/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde::de::{self, Deserialize, Deserializer, Visitor, MapAccess};
use serde::ser::{Serialize, Serializer, SerializeStruct};

use serde_json;
use std::marker::PhantomData;
use std::ord::Ord;

use error;
use key_bundle::KeyBundle;

#[derive(Debug, Clone, Deserialize)]
pub struct BsoRecord<T> where T: Serialize + Deserialize {
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
    pub payload: T,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct EncryptedPayload {
    #[serde(rename = "IV")]
    pub iv: String,
    pub ciphertext: String,
    pub hmac: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct MetaGlobalEngine {
    pub version: usize,
    #[serde(rename = "syncID")]
    pub sync_id: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct MetaGlobalPayload {
    #[serde(rename = "syncID")]
    pub sync_id: String,
    #[serde(rename = "storageVersion")]
    pub storage_version: usize,
    pub declined: Vec<String>,
    pub engines: HashMap<String, MetaGlobalEngine>,
}

pub type EncryptedRecord = BsoRecord<EncryptedPayload>;
pub type MetaGlobalRecord = BsoRecord<MetaGlobalPayload>;

// Custom deserializer to handle auto-deserializing the payload from JSON.
fn deserialize_json<'de, T, D>(deserializer: D) -> Result<T, D::Error> where T: Deserialize<'de>, D: Deserializer<'de> {
    struct DeserializeNestedJson<T>(PhantomData<fn() -> T>);

    impl<'de, T> Visitor<'de> for DeserializeNestedJson<T> where T: Deserialize<'de> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("The JSON-encoded payload as string")
        }

        fn visit_str<E>(self, value: &str) -> Result<T, E> where E: de::Error {
            serde_json::from_str(value)
        }
    }

    let visitor = DeserializeNestedJson(PhantomData);
    deserializer.deserialize_str(visitor)
}

// Custom serializer to handle auto-serializing the payload to JSON
impl<T> Serialize for BsoRecord<T> where T: Serialize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
        // Serialize the object we hold in our payload to a string right away
        let payload_json = serde_json::to_string(self.payload)?;

        // We always serialize id and payload, and serialize collection, ttl, and sortindex iff.
        // they are present. Annoyingly, serialize_struct requires us tell how many we'll serialize
        // up-front.
        let num_fields = 2 + (self.collection.is_some() as usize)
                           + (self.ttl.is_some() as usize)
                           + (self.sortindex.is_some() as usize);

        // Note: The name here doesn't show up in the output. At least, not for JSON.
        let mut state = serializer.serialize_struct("BsoRecord", num_fields)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("payload", payload)?;

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







/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use super::Guid;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};

/// Represents the decrypted payload in a Bso. Provides a minimal layer of type
/// safety to avoid double-encrypting.
///
/// Note: If we implement a full sync client in rust we may want to consider
/// using stronger types for each record (we did this in the past as well), but
/// for now, since everything is just going over the FFI, there's not a lot of
/// benefit here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Payload {
    pub id: Guid,

    #[serde(default)]
    #[serde(skip_serializing_if = "crate::skip_if_default")]
    pub deleted: bool,

    #[serde(flatten)]
    pub data: Map<String, JsonValue>,
}

impl Payload {
    pub fn new_tombstone(id: impl Into<Guid>) -> Payload {
        Payload {
            id: id.into(),
            deleted: true,
            data: Map::new(),
        }
    }

    pub fn new_tombstone_with_ttl(id: impl Into<Guid>, ttl: u32) -> Payload {
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

    pub fn from_json(value: JsonValue) -> Result<Payload, serde_json::Error> {
        serde_json::from_value(value)
    }

    pub fn into_record<T>(self) -> Result<T, serde_json::Error>
    where
        for<'a> T: Deserialize<'a>,
    {
        serde_json::from_value(JsonValue::from(self))
    }

    pub fn from_record<T: Serialize>(v: T) -> Result<Payload, serde_json::Error> {
        // TODO: This is dumb, we do to_value and then from_value. If we end up using this
        // method a lot we should rethink... As it is it should just be for uploading
        // meta/global or crypto/keys which is rare enough that it doesn't matter.
        Ok(Payload::from_json(serde_json::to_value(v)?)?)
    }

    pub fn into_json_string(self) -> String {
        serde_json::to_string(&JsonValue::from(self))
            .expect("JSON.stringify failed, which shouldn't be possible")
    }
}

impl From<Payload> for JsonValue {
    fn from(cleartext: Payload) -> Self {
        let Payload {
            mut data,
            id,
            deleted,
        } = cleartext;
        data.insert("id".to_string(), JsonValue::String(id.into_string()));
        if deleted {
            data.insert("deleted".to_string(), JsonValue::Bool(true));
        }
        JsonValue::Object(data)
    }
}

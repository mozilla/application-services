/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use bso_record::{BsoRecord, Sync15Record};
use std::collections::HashMap;

pub use MaybeTombstone::*;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum MaybeTombstone<T> {
    Tombstone { id: String, deleted: bool },
    NonTombstone(T)
}

impl<T> MaybeTombstone<T> {
    #[inline]
    pub fn tombstone<R: Into<String>>(id: R) -> MaybeTombstone<T> {
        Tombstone { id: id.into(), deleted: true }
    }

    #[inline]
    pub fn is_tombstone(&self) -> bool {
        match self {
            &NonTombstone(_) => false,
            _ => true
        }
    }

    #[inline]
    pub fn unwrap(self) -> T {
        match self {
            NonTombstone(record) => record,
            _ => panic!("called `MaybeTombstone::unwrap()` on a Tombstone!"),
        }
    }

    #[inline]
    pub fn expect(self, msg: &str) -> T {
        match self {
            NonTombstone(record) => record,
            _ => panic!("{}", msg),
        }
    }

    #[inline]
    pub fn ok_or<E>(self, err: E) -> ::std::result::Result<T, E> {
        match self {
            NonTombstone(record) => Ok(record),
            _ => Err(err)
        }
    }

    #[inline]
    pub fn record(self) -> Option<T> {
        match self {
            NonTombstone(record) => Some(record),
            _ => None
        }
    }
}

impl<T> Sync15Record for MaybeTombstone<T> where T: Sync15Record {
    fn collection_tag() -> &'static str { T::collection_tag() }
    fn ttl() -> Option<u32> { T::ttl() }
    fn record_id(&self) -> &str {
        match self {
            &Tombstone { ref id, .. } => id,
            &NonTombstone(ref record) => record.record_id()
        }
    }
    fn sortindex(&self) -> Option<i32> {
        match self {
            &Tombstone { .. } => None,
            &NonTombstone(ref record) => record.sortindex()
        }
    }
}

impl<T> BsoRecord<MaybeTombstone<T>> {
    #[inline]
    pub fn is_tombstone(&self) -> bool {
        self.payload.is_tombstone()
    }

    #[inline]
    pub fn record(self) -> Option<BsoRecord<T>> where T: Clone {
        self.map_payload(|payload| payload.record()).transpose()
    }
}

pub type MaybeTombstoneRecord<T> = BsoRecord<MaybeTombstone<T>>;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordRecord {
    pub id: String,
    pub hostname: Option<String>,

    // rename_all = "camelCase" by default will do formSubmitUrl, but we can just
    // override this one field.
    #[serde(rename = "formSubmitURL")]
    pub form_submit_url: Option<String>,

    pub http_realm: Option<String>,

    #[serde(default = "String::new")]
    pub username: String,

    pub password: String,

    #[serde(default = "String::new")]
    pub username_field: String,

    #[serde(default = "String::new")]
    pub password_field: String,

    pub time_created: i64,
    pub time_password_changed: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_last_used: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub times_used: Option<i64>,
}

impl Sync15Record for PasswordRecord {
    fn collection_tag() -> &'static str { "passwords" }
    fn record_id(&self) -> &str { &self.id }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetaGlobalEngine {
    pub version: usize,
    #[serde(rename = "syncID")]
    pub sync_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetaGlobalRecord {
    #[serde(rename = "syncID")]
    pub sync_id: String,
    #[serde(rename = "storageVersion")]
    pub storage_version: usize,
    pub engines: HashMap<String, MetaGlobalEngine>,
    pub declined: Vec<String>,
}

impl Sync15Record for MetaGlobalRecord {
    fn collection_tag() -> &'static str { "meta" }
    fn record_id(&self) -> &str { "global" }
}

#[cfg(test)]
mod tests {

    use super::*;
    use key_bundle::KeyBundle;
    use util::ServerTimestamp;
    use serde_json;

    #[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
    struct DummyRecord {
        id: String,
        age: i64,
        meta: String,
    }

    impl Sync15Record for DummyRecord {
        fn collection_tag() -> &'static str { "dummy" }
        fn record_id(&self) -> &str { &self.id }
    }

    #[test]
    fn test_roundtrip_crypt_tombstone() {
        let orig_record: MaybeTombstoneRecord<DummyRecord> = BsoRecord {
            id: "aaaaaaaaaaaa".into(),
            collection: "dummy".into(),
            modified: ServerTimestamp(1234.0),
            sortindex: None,
            ttl: None,
            payload: MaybeTombstone::tombstone("aaaaaaaaaaaa")
        };

        assert!(orig_record.is_tombstone());

        let keybundle = KeyBundle::new_random().unwrap();

        let encrypted = orig_record.clone().encrypt(&keybundle).unwrap();

        assert!(keybundle.verify_hmac_string(
            &encrypted.payload.hmac, &encrypted.payload.ciphertext).unwrap());

        // While we're here, check on EncryptedPayload::serialized_len
        let val_rec = serde_json::from_str::<serde_json::Value>(
            &serde_json::to_string(&encrypted).unwrap()).unwrap();
        assert_eq!(encrypted.payload.serialized_len(),
                   val_rec["payload"].as_str().unwrap().len());

        let decrypted: MaybeTombstoneRecord<DummyRecord> = encrypted.decrypt(&keybundle).unwrap();
        assert!(decrypted.is_tombstone());
        assert_eq!(decrypted, orig_record);
    }

    #[test]
    fn test_roundtrip_crypt_record() {
        let orig_record: MaybeTombstoneRecord<DummyRecord> = BsoRecord {
            id: "aaaaaaaaaaaa".into(),
            collection: "dummy".into(),
            modified: ServerTimestamp(1234.0),
            sortindex: None,
            ttl: None,
            payload: NonTombstone(DummyRecord {
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

        // While we're here, check on EncryptedPayload::serialized_len
        let val_rec = serde_json::from_str::<serde_json::Value>(
            &serde_json::to_string(&encrypted).unwrap()).unwrap();
        assert_eq!(encrypted.payload.serialized_len(),
                   val_rec["payload"].as_str().unwrap().len());

        let decrypted: MaybeTombstoneRecord<DummyRecord> = encrypted.decrypt(&keybundle).unwrap();
        assert!(!decrypted.is_tombstone());
        assert_eq!(decrypted, orig_record);
    }
}

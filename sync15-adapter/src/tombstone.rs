/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use bso_record::{BsoRecord, Sync15Record};

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
            &Tombstone { ref id, .. } => id.as_str(),
            &NonTombstone(ref record) => record.record_id(),
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

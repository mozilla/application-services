/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use bso_record::{BsoRecord, Sync15Record, EncryptedPayload};
use key_bundle::KeyBundle;
use std::collections::HashMap;
use error::Result;

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
struct CryptoKeysRecord {
    pub id: String,
    pub collection: String,
    pub default: [String; 2],
    pub collections: HashMap<String, [String; 2]>
}

impl Sync15Record for CryptoKeysRecord {
    fn collection_tag() -> &'static str { "crypto" }
    fn record_id(&self) -> &str { "keys" }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionKeys {
    pub default: KeyBundle,
    pub collections: HashMap<String, KeyBundle>
}

impl CollectionKeys {
    pub fn from_encrypted_bso(record: BsoRecord<EncryptedPayload>, root_key: &KeyBundle) -> Result<CollectionKeys> {
        let keys: BsoRecord<CryptoKeysRecord> = record.decrypt(root_key)?;
        Ok(CollectionKeys {
            default: KeyBundle::from_base64(&keys.payload.default[0], &keys.payload.default[1])?,
            collections:
                keys.payload.collections
                              .into_iter()
                              .map(|kv| Ok((kv.0, KeyBundle::from_base64(&kv.1[0], &kv.1[1])?)))
                              .collect::<Result<HashMap<String, KeyBundle>>>()?
        })
    }

    fn to_bso(&self) -> BsoRecord<CryptoKeysRecord> {
        CryptoKeysRecord {
            id: "keys".into(),
            collection: "crypto".into(),
            default: self.default.to_b64_array(),
            collections: self.collections.iter().map(|kv|
                (kv.0.clone(), kv.1.to_b64_array())).collect()
        }.into()
    }

    #[inline]
    pub fn to_encrypted_bso(&self, root_key: &KeyBundle) -> Result<BsoRecord<EncryptedPayload>> {
        self.to_bso().encrypt(root_key)
    }

    #[inline]
    pub fn key_for_collection<'a>(&'a self, collection: &str) -> &'a KeyBundle {
        self.collections.get(collection).unwrap_or(&self.default)
    }
}

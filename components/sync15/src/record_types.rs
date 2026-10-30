/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde_derive::*;
use std::collections::HashMap;
use sync_guid::Guid;

// Known record formats.

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetaGlobalEngine {
    pub version: usize,
    #[serde(rename = "syncID")]
    pub sync_id: Guid,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetaGlobalRecord {
    #[serde(rename = "syncID")]
    pub sync_id: Guid,
    #[serde(rename = "storageVersion")]
    pub storage_version: usize,
    #[serde(default)]
    pub engines: HashMap<String, MetaGlobalEngine>,
    #[serde(default)]
    pub declined: Vec<String>,
}

/// The only version of `meta/syncids` we understand.
pub const SYNC_IDS_RECORD_VERSION: usize = 1;

/// The `meta/syncids` record, a registry of per-engine sync IDs written by
/// clients which allow engines to be enabled or disabled per device. Such
/// clients don't add engines to `meta/global`, so this is where we look for an
/// engine's sync ID before minting a new one - minting would force all of them
/// to reset the engine.
/// We only ever read it.
#[derive(Deserialize, Debug, Clone)]
pub struct SyncIdsRecord {
    #[serde(default)]
    pub version: usize,
    /// The `meta/global` sync ID this record was written under.
    #[serde(rename = "globalSyncID", default)]
    pub global_sync_id: Option<Guid>,
    #[serde(default)]
    pub engines: HashMap<String, MetaGlobalEngine>,
}

impl SyncIdsRecord {
    /// Returns the entry for an engine if it's safe to adopt it for the given
    /// `meta/global` and engine version. These rules must match Desktop's.
    pub fn usable_entry(
        &self,
        global: &MetaGlobalRecord,
        name: &str,
        version: usize,
    ) -> Option<&MetaGlobalEngine> {
        if self.version != SYNC_IDS_RECORD_VERSION
            || self.global_sync_id.as_ref() != Some(&global.sync_id)
        {
            return None;
        }
        self.engines
            .get(name)
            .filter(|e| e.version == version && !e.sync_id.is_empty())
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Eq, PartialEq)]
pub struct CryptoKeysRecord {
    pub id: Guid,
    pub collection: String,
    pub default: [String; 2],
    pub collections: HashMap<String, [String; 2]>,
}

#[cfg(test)]
#[test]
fn test_deserialize_meta_global() {
    let record = serde_json::json!({
        "syncID": "abcd1234abcd",
        "storageVersion": 1,
    })
    .to_string();
    let r = serde_json::from_str::<MetaGlobalRecord>(&record).unwrap();
    assert_eq!(r.sync_id, "abcd1234abcd");
    assert_eq!(r.storage_version, 1);
    assert_eq!(r.engines.len(), 0);
    assert_eq!(r.declined.len(), 0);
}

#[cfg(test)]
mod sync_ids_tests {
    use super::*;

    fn global() -> MetaGlobalRecord {
        MetaGlobalRecord {
            sync_id: "globalAAAAAA".into(),
            storage_version: 5,
            engines: HashMap::new(),
            declined: vec![],
        }
    }

    fn parse(v: serde_json::Value) -> SyncIdsRecord {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn test_usable_entry() {
        let r = parse(serde_json::json!({
            "version": 1,
            "globalSyncID": "globalAAAAAA",
            "engines": {
                "bookmarks": {"version": 2, "syncID": "bookmarksAAA"},
                "empty": {"version": 1, "syncID": ""},
            },
            "someFutureField": true,
        }));
        let entry = r.usable_entry(&global(), "bookmarks", 2).unwrap();
        assert_eq!(entry.sync_id, "bookmarksAAA");
        // wrong engine version
        assert!(r.usable_entry(&global(), "bookmarks", 1).is_none());
        // no such engine
        assert!(r.usable_entry(&global(), "history", 1).is_none());
        // empty syncID
        assert!(r.usable_entry(&global(), "empty", 1).is_none());
    }

    #[test]
    fn test_unusable_records() {
        let engines = serde_json::json!({"bookmarks": {"version": 2, "syncID": "bookmarksAAA"}});
        for v in [
            // another account (or another generation of this one)
            serde_json::json!({"version": 1, "globalSyncID": "globalBBBBBB", "engines": engines}),
            // missing globalSyncID
            serde_json::json!({"version": 1, "engines": engines}),
            // unknown record versions
            serde_json::json!({"version": 2, "globalSyncID": "globalAAAAAA", "engines": engines}),
            serde_json::json!({"globalSyncID": "globalAAAAAA", "engines": engines}),
        ] {
            assert!(parse(v).usable_entry(&global(), "bookmarks", 2).is_none());
        }
        // and no engines at all is fine.
        let r = parse(serde_json::json!({"version": 1, "globalSyncID": "globalAAAAAA"}));
        assert!(r.usable_entry(&global(), "bookmarks", 2).is_none());
    }
}

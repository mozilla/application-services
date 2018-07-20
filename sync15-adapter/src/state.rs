/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;

use bso_record::BsoRecord;
use collection_keys::CollectionKeys;
use error::{self, ErrorKind};
use key_bundle::KeyBundle;
use record_types::MetaGlobalRecord;
use request::InfoConfiguration;
use util::{ServerTimestamp, SERVER_EPOCH};

/// Holds global Sync state, including server upload limits, and the
/// last-fetched collection modified times, `meta/global` record, and
/// collection encryption keys.
#[derive(Debug)]
pub struct GlobalState {
    pub config: InfoConfiguration,
    pub collections: HashMap<String, ServerTimestamp>,
    pub global: Option<BsoRecord<MetaGlobalRecord>>,
    pub keys: Option<CollectionKeys>,
}

impl Default for GlobalState {
    fn default() -> Self {
        GlobalState {
            config: InfoConfiguration::default(),
            collections: HashMap::new(),
            global: None,
            keys: None,
        }
    }
}

impl GlobalState {
    pub fn key_for_collection(&self, collection: &str) -> error::Result<&KeyBundle> {
        Ok(self.keys
            .as_ref()
            .ok_or_else(|| ErrorKind::NoCryptoKeys)?
            .key_for_collection(collection))
    }

    pub fn last_modified_or_zero(&self, coll: &str) -> ServerTimestamp {
        self.collections.get(coll).cloned().unwrap_or(SERVER_EPOCH)
    }
}

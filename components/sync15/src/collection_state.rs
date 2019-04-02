/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error;
use crate::key_bundle::KeyBundle;
use crate::request::InfoConfiguration;
use crate::state::GlobalState;
use crate::sync::Store;
use crate::util::ServerTimestamp;

/// Holds state for a collection. In general, only the CollectionState is
/// needed to sync a collection (but a valid GlobalState is needed to obtain
/// a CollectionState)
#[derive(Debug, Clone)]
pub struct CollectionState {
    pub config: InfoConfiguration,
    // initially from meta/global, updated after an xius POST/PUT.
    pub last_modified: ServerTimestamp,
    pub key: KeyBundle,
}

pub enum LocalCollectionState {
    Unknown {
        global_sync_id: Option<String>,
        store_sync_id: Option<String>,
    },
    Declined,
    GlobalSyncIdChanged,
    StoreSyncIdChanged {
        global_sync_id: String,
    },
    Ready {
        key: KeyBundle,
        global_sync_id: String,
        store_sync_id: String,
    },
}

pub struct LocalCollectionStateMachine<'state> {
    global_state: &'state GlobalState,
}

impl<'state> LocalCollectionStateMachine<'state> {
    fn advance(
        &self,
        from: LocalCollectionState,
        store: &Store,
    ) -> error::Result<LocalCollectionState> {
        let name = &store.collection_name().to_string();
        let meta_global = &self.global_state.global;
        match from {
            LocalCollectionState::Unknown {
                global_sync_id: maybe_gsid,
                store_sync_id: maybe_ssid,
            } => {
                if meta_global.declined.contains(name) {
                    return Ok(LocalCollectionState::Declined);
                }
                let global_sync_id = meta_global.sync_id.clone();
                if maybe_gsid.map(|id| id != global_sync_id).unwrap_or(true) {
                    return Ok(LocalCollectionState::GlobalSyncIdChanged);
                }
                let store_sync_id = meta_global.engines[name].sync_id.clone();
                if maybe_ssid.map(|id| id != store_sync_id).unwrap_or(true) {
                    return Ok(LocalCollectionState::StoreSyncIdChanged { global_sync_id });
                }
                return Ok(LocalCollectionState::Ready {
                    key: self.global_state.keys.default.clone(),
                    global_sync_id,
                    store_sync_id,
                });
            }

            LocalCollectionState::Declined => unreachable!("can't advance from declined"),

            LocalCollectionState::GlobalSyncIdChanged
            | LocalCollectionState::StoreSyncIdChanged { .. } => {
                // we treat either guid changing the same way - we already have
                // a new meta/global with new IDs - so grab them both and advance.
                let global_sync_id = meta_global.sync_id.clone();
                let store_sync_id = meta_global.engines[name].sync_id.clone(); // xxx - how does `[name]` not potentially fail?
                store.reset(&global_sync_id, &store_sync_id)?;
                Ok(LocalCollectionState::Unknown {
                    global_sync_id: Some(global_sync_id),
                    store_sync_id: Some(store_sync_id),
                })
            }

            LocalCollectionState::Ready { .. } => unreachable!("can't advance from ready"),
        }
    }

    fn run_and_run_as_farst_as_you_can(
        &mut self,
        store: &Store,
    ) -> error::Result<Option<CollectionState>> {
        let (global_sync_id, store_sync_id) = store.get_sync_ids()?;
        let mut s = LocalCollectionState::Unknown {
            global_sync_id,
            store_sync_id,
        };
        loop {
            match s {
                LocalCollectionState::Ready { key, .. } => {
                    let name = store.collection_name();
                    let config = self.global_state.config.clone();
                    let last_modified = self
                        .global_state
                        .collections
                        .get(name)
                        .cloned()
                        .unwrap_or_default();
                    return Ok(Some(CollectionState {
                        config,
                        last_modified,
                        key,
                    }));
                }
                LocalCollectionState::Declined => return Ok(None),

                _ => {
                    // XXX - loop detection?
                    s = self.advance(s, store)?;
                }
            };
        }
    }

    pub fn get_state(
        store: &Store,
        global_state: &'state GlobalState,
    ) -> error::Result<Option<CollectionState>> {
        let mut gingerbread_man = Self { global_state };
        gingerbread_man.run_and_run_as_farst_as_you_can(store)
    }
}

/*

TODO - tests

#[cfg(test)]
mod tests {
    use super::*;

    fn get_global_state() -> GlobalState {
        GlobalState {
            config: InfoConfiguration::default(),
            collections: TimestampedResponse {
                record: InfoCollections::new(),
                last_modified: 123.4.into(),
            },
            global: TimestampedResponse {
                record: MetaGlobalRecord::default(),
                last_modified: 567.8.into(),
            },
            keys: CollectionKeys::new_random(),
        }
    }

    struct TestStore {

    }
    impl Store for TestStore {

    }

    #[test]
    fn test_something() {

    }
}
*/

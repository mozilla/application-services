/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use bso_record::{BsoRecord, EncryptedPayload};
use collection_keys::CollectionKeys;
use error;
use key_bundle::KeyBundle;
use record_types::{MetaGlobalEngine, MetaGlobalRecord};
use request::InfoConfiguration;
use storage_client::StorageClient;
use util::ServerTimestamp;

use self::SyncState::*;

const STORAGE_VERSION: usize = 5;

/// A storage client trait to make testing easier.
trait Client {
    fn fetch_info_configuration(&self) -> error::Result<InfoConfiguration>;
    fn fetch_info_collections(&self) -> error::Result<HashMap<String, ServerTimestamp>>;
    fn fetch_meta_global(&self) -> error::Result<BsoRecord<MetaGlobalRecord>>;
    fn fetch_crypto_keys(&self) -> error::Result<BsoRecord<EncryptedPayload>>;
    fn wipe_storage(&self) -> error::Result<()>;
}

enum EngineStateChange {
    ResetAll { except: HashSet<String> },
    Enable(String),
    Disable(String),
    Reset(String),
}

enum MetaGlobalEnginesAction {
    ResetAll,
    ChangeStates(Vec<EngineStateChange>),
}

/// A scratchpad persists and caches Sync state, as on iOS.

// TODO: Is there a better abstraction here?
trait Scratchpad {
    /// Fetches locally cached server limits.
    fn config(&self) -> &InfoConfiguration;

    /// Stores new server limits from `info/configuration`.
    fn set_config(&mut self, config: InfoConfiguration);

    /// Fetches the current `meta/global` record, or synthesizes a record that
    /// reflects local state. Note that the record doesn't need to be persisted
    /// in one piece. For example, a scratchpad could store the global
    /// `sync_id`, and the `sync_id`s for bookmarks and passwords, in completely
    /// separate stores, and synthesize a `meta/global` record with fields
    /// pulled from these databases.
    fn global(&self) -> Option<&BsoRecord<MetaGlobalRecord>>;

    /// Stores a `meta/global` record from the server.
    fn set_global(&mut self, global: Option<BsoRecord<MetaGlobalRecord>>);

    /// Fetches the current `crypto/keys` record.
    fn keys(&self) -> Option<&CollectionKeys>;

    /// Stores a `crypto/keys` record from the server.
    fn set_keys(&mut self, keys: Option<CollectionKeys>);

    /// Adds an engine state change to process once we're ready to sync.
    fn add_engine_state_change(&mut self, change: EngineStateChange);

    /// Adds engine state changes for any engines with new collection keys.
    // If the default keys change, reset all engines, except those that have the
    // same collection-specific keys. If the default keys are the same, only
    // reset engines with different collection-specific keys.
    fn add_engine_state_changes_from_keys(&mut self, keys: Option<&CollectionKeys>) {
        let action = match keys {
            Some(fresh_keys) => {
                match self.keys() {
                    Some(stale_keys) => {
                        let mut changes = Vec::new();
                        if fresh_keys.default == stale_keys.default {
                            // The default bundle is the same, so only reset
                            // engines with different collection-specific keys.
                            for (collection, key_bundle) in &stale_keys.collections {
                                if key_bundle != fresh_keys.key_for_collection(collection) {
                                    changes.push(EngineStateChange::Reset(collection.to_string()));
                                }
                            }
                            for (collection, key_bundle) in &fresh_keys.collections {
                                if key_bundle != stale_keys.key_for_collection(collection) {
                                    changes.push(EngineStateChange::Reset(collection.to_string()));
                                }
                            }
                        } else {
                            // The default bundle changed, so reset all engines
                            // except those with the same collection-specific
                            // keys.
                            let mut except = HashSet::new();
                            for (collection, key_bundle) in &stale_keys.collections {
                                if key_bundle == fresh_keys.key_for_collection(collection) {
                                    except.insert(collection.to_string());
                                }
                            }
                            for (collection, key_bundle) in &fresh_keys.collections {
                                if key_bundle != stale_keys.key_for_collection(collection) {
                                    except.insert(collection.to_string());
                                }
                            }
                        }
                        MetaGlobalEnginesAction::ChangeStates(changes)
                    },
                    None => MetaGlobalEnginesAction::ResetAll
                }
            },
            None => MetaGlobalEnginesAction::ResetAll
        };
        match action {
            MetaGlobalEnginesAction::ResetAll => self.add_engine_state_change(EngineStateChange::ResetAll { except: HashSet::new() }),
            MetaGlobalEnginesAction::ChangeStates(changes) => {
                for change in changes {
                    self.add_engine_state_change(change);
                }
            }
        }
    }

    /// Persists the scratchpad. This method intentionally doesn't return an
    /// error, since we don't want metadata storage errors to block the client
    /// from syncing, and we can keep the new `meta/global` and `crypto/keys`
    /// in memory. (We'll just re-fetch them on the first sync after the
    /// next startup).
    fn save(&mut self);
}

// TODO: Could we generalize this to fetch a token from the token server,
// and simplify the token server client a bit? If the storage node changes,
// we'll need to go through the entire process again, but we can skip a few
// states if the node matches. Maybe we could persist the endpoint and token
// in the scratchpad (token could be kept in-memory only), in addition to
// `meta/global` and `crypto/keys`, and add some states for "needs fresh
// token" and "has fresh token, check if endpoint changed". If thee endpoint is
// the same, continue to `InitialWithLiveToken`; if different, go to
// `FreshStartRequired`.

// TODO: Should we expose this over FFI? I think not, we want the library to
// automatically handle getting a new token and setting everything up so that
// the app can just sync.
struct SyncStateMachine<'s, 'k> {
    client: Rc<Client>,
    scratchpad: &'s mut Scratchpad,
    keys: &'k KeyBundle,
    label_sequence: Vec<&'static str>,
}

impl<'s, 'k> SyncStateMachine<'s, 'k> {
    fn new(
        client: Rc<Client>,
        scratchpad: &'s mut Scratchpad,
        keys: &'k KeyBundle,
    ) -> SyncStateMachine<'s, 'k> {
        SyncStateMachine {
            client,
            scratchpad,
            keys,
            label_sequence: Vec::new(),
        }
    }

    // Runs through the state machine to the ready state.
    fn to_ready(&mut self) -> error::Result<()> {
        let mut state = InitialWithLiveToken;
        loop {
            match &state {
                Ready => {
                    self.label_sequence.push(&state.label());
                    break;
                }
                // If we already started over once before, we're likely in a
                // cycle, and should try again later. Like the iOS state
                // machine, other cycles aren't a problem; we'll cycle through
                // earlier states if we need to reupload `meta/global` or
                // `crypto/keys`.
                FreshStartRequired if self.label_sequence.contains(&state.label()) => {
                    bail!(error::unexpected("State machine cycle error"))
                }
                _ => {
                    self.label_sequence.push(&state.label());
                    state = state.advance(self.client.clone(), self.scratchpad, &self.keys)?;
                }
            }
        }
        Ok(())
    }
}

/// Whether we should skip fetching `meta/global` or `crypto/keys` from the
/// server because our locally cached copy is up-to-date, fetch a fresh copy
/// from the server, or invalidate cached state, then fetch.
enum FetchAction {
    Skip,
    Fetch,
    InvalidateThenFetch,
}

enum SyncState {
    InitialWithLiveToken,
    InitialWithLiveTokenAndInfo {
        collections: HashMap<String, ServerTimestamp>,
    },
    NeedsFreshMetaGlobal {
        collections: HashMap<String, ServerTimestamp>,
    },
    HasMetaGlobal {
        collections: HashMap<String, ServerTimestamp>,
    },
    ResolveMetaGlobal {
        collections: HashMap<String, ServerTimestamp>,
        global: BsoRecord<MetaGlobalRecord>,
    },
    NeedsFreshCryptoKeys,
    Ready,
    FreshStartRequired,
}

impl SyncState {
    fn label(&self) -> &'static str {
        match self {
            // TODO: This could probably be cleaned up with a macro.
            InitialWithLiveToken { .. } => "InitialWithLiveToken",
            InitialWithLiveTokenAndInfo { .. } => "InitialWithLiveTokenAndInfo",
            NeedsFreshMetaGlobal { .. } => "NeedsFreshMetaGlobal",
            HasMetaGlobal { .. } => "HasMetaGlobal",
            ResolveMetaGlobal { .. } => "ResolveMetaGlobal",
            NeedsFreshCryptoKeys => "NeedsFreshCryptoKeys",
            Ready => "Ready",
            FreshStartRequired => "FreshStartRequired",
        }
    }

    fn advance(
        self,
        client: Rc<Client>,
        scratchpad: &mut Scratchpad,
        root_key: &KeyBundle,
    ) -> error::Result<SyncState> {
        match self {
            // Fetch `info/configuration` with current server limits, and
            // `info/collections` with collection last modified times.
            InitialWithLiveToken => {
                let config = client.fetch_info_configuration()?;
                scratchpad.set_config(config);

                let collections = client.fetch_info_collections()?;
                Ok(InitialWithLiveTokenAndInfo { collections })
            }

            // Compare local and remote `meta/global` timestamps to determine
            // if our locally cached `meta/global` is up-to-date.
            InitialWithLiveTokenAndInfo { collections } => {
                let action = match scratchpad.global() {
                    Some(global) => match &collections.get("meta") {
                        Some(modified) => {
                            if global.modified >= **modified {
                                FetchAction::Skip
                            } else {
                                FetchAction::Fetch
                            }
                        }
                        None => FetchAction::InvalidateThenFetch,
                    },
                    None => FetchAction::Fetch,
                };
                Ok(match action {
                    // Hooray, we don't need to fetch `meta/global`. Skip to
                    // the next state.
                    FetchAction::Skip => HasMetaGlobal { collections },
                    // Our `meta/global` is out of date, or isn't cached
                    // locally, so we need to fetch it from the server.
                    FetchAction::Fetch => NeedsFreshMetaGlobal { collections },
                    // We have a `meta/global` record in our cache, but not on
                    // the server. This likely means we're the first client to
                    // sync after a node reassignment. Invalidate our cached
                    // `meta/global` and `crypto/keys`, and try to fetch
                    // `meta/global` from the server anyway. If another client
                    // wins the race, we'll fetch its `meta/global`; if not,
                    // we'll fail and upload our own.
                    FetchAction::InvalidateThenFetch => {
                        scratchpad.set_global(None);
                        scratchpad.set_keys(None);
                        scratchpad.save();
                        NeedsFreshMetaGlobal { collections }
                    }
                })
            }

            // Fetch `meta/global` from the server.
            NeedsFreshMetaGlobal { collections } => {
                match client.fetch_meta_global() {
                    Ok(global) => Ok(ResolveMetaGlobal {
                        collections,
                        global,
                    }),
                    // If the server doesn't have a `meta/global`, start over.
                    Err(error::Error(error::ErrorKind::NoMetaGlobal, _)) => Ok(FreshStartRequired),
                    Err(err) => Err(err),
                }
            }

            // Reconcile the server's `meta/global` with our locally cached
            // `meta/global`, if any.
            ResolveMetaGlobal {
                collections,
                global,
            } => {
                // If the server has a newer storage version, we can't
                // sync until our client is updated.
                if &global.payload.storage_version > &STORAGE_VERSION {
                    bail!(error::unexpected("Client upgrade required"))
                }

                // If the server has an older storage version, wipe and
                // reupload.
                if &global.payload.storage_version < &STORAGE_VERSION {
                    return Ok(FreshStartRequired);
                }

                let action = match &scratchpad.global() {
                    Some(previous_global) => {
                        if &previous_global.sync_id != &global.sync_id {
                            MetaGlobalEnginesAction::ResetAll
                        } else {
                            let mut changes = Vec::new();
                            let previous_engine_names =
                                previous_global.engines.keys().collect::<HashSet<&String>>();
                            let current_engine_names =
                                global.engines.keys().collect::<HashSet<&String>>();

                            // Disable any local engines that aren't mentioned
                            // in the new `meta/global`.
                            for name in previous_engine_names.difference(&current_engine_names) {
                                changes.push(EngineStateChange::Disable(name.to_string()));
                            }

                            // Enable any new engines that aren't mentioned in
                            // the locally cached `meta/global`.
                            for name in current_engine_names.difference(&previous_engine_names) {
                                changes.push(EngineStateChange::Enable(name.to_string()));
                            }

                            // Reset engines with sync ID changes.
                            for name in current_engine_names.intersection(&previous_engine_names) {
                                // does Rust support multiple `if let Some(k) = x, let Some(l) = y { ... }
                                let previous_engine = previous_global.engines.get(*name).unwrap();
                                let engine = global.engines.get(*name).unwrap();
                                if previous_engine.sync_id != engine.sync_id {
                                    changes.push(EngineStateChange::Reset(name.to_string()));
                                }
                            }
                            MetaGlobalEnginesAction::ChangeStates(changes)
                        }
                    }
                    None => MetaGlobalEnginesAction::ResetAll,
                };

                match action {
                    MetaGlobalEnginesAction::ResetAll => {
                        scratchpad.set_global(Some(global));
                        scratchpad.set_keys(None);
                        scratchpad.add_engine_state_changes_from_keys(None);
                        scratchpad.save();
                        Ok(HasMetaGlobal { collections })
                    }
                    MetaGlobalEnginesAction::ChangeStates(changes) => {
                        scratchpad.set_global(Some(global));
                        for change in changes {
                            scratchpad.add_engine_state_change(change);
                        }
                        scratchpad.save();
                        Ok(HasMetaGlobal { collections })
                    }
                }
            }

            // Check if our locally cached `crypto/keys` collection is
            // up-to-date.
            HasMetaGlobal { collections } => {
                let action = match scratchpad.keys() {
                    Some(keys) => match &collections.get("crypto") {
                        Some(modified) => {
                            if keys.modified >= **modified {
                                FetchAction::Skip
                            } else {
                                FetchAction::Fetch
                            }
                        }
                        None => FetchAction::InvalidateThenFetch,
                    },
                    None => FetchAction::Fetch,
                };
                Ok(match action {
                    // If `crypto/keys` is up-to-date, we're ready to go!
                    FetchAction::Skip => Ready,
                    // We need to fetch and cache new keys.
                    FetchAction::Fetch => NeedsFreshCryptoKeys,
                    // We need to invalidate our locally cached `crypto/keys`,
                    // then try to fetch new keys, and reupload if fetching
                    // fails.
                    FetchAction::InvalidateThenFetch => {
                        scratchpad.set_keys(None);
                        NeedsFreshCryptoKeys
                    }
                })
            }

            NeedsFreshCryptoKeys => {
                match client.fetch_crypto_keys() {
                    Ok(encrypted_bso) => {
                        let keys = CollectionKeys::from_encrypted_bso(encrypted_bso, root_key)?;
                        scratchpad.add_engine_state_changes_from_keys(Some(&keys));
                        scratchpad.set_keys(Some(keys));
                        Ok(Ready)
                    }
                    // If the server doesn't have a `crypto/keys`, start over
                    // and reupload our `meta/global` and `crypto/keys`.
                    Err(error::Error(error::ErrorKind::NoCryptoKeys, _)) => Ok(FreshStartRequired),
                    Err(err) => Err(err),
                }
            }

            Ready => bail!(error::unexpected("Can't advance past ready")),

            FreshStartRequired => {
                client.wipe_storage()?;
                scratchpad.set_global(None);
                scratchpad.add_engine_state_changes_from_keys(None);
                scratchpad.set_keys(None);
                bail!(error::unexpected(
                    "Need to upload new m/g with existing engine configuration and random c/k"
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct InMemoryClient {
        info_configuration: error::Result<InfoConfiguration>,
        info_collections: error::Result<HashMap<String, ServerTimestamp>>,
        meta_global: error::Result<BsoRecord<MetaGlobalRecord>>,
        crypto_keys: error::Result<BsoRecord<EncryptedPayload>>,
    }

    impl Client for InMemoryClient {
        fn fetch_info_configuration(&self) -> error::Result<InfoConfiguration> {
            match &self.info_configuration {
                Ok(config) => Ok(config.clone()),
                Err(err) => Err(error::unexpected("Can't fetch info/configuration")),
            }
        }

        fn fetch_info_collections(&self) -> error::Result<HashMap<String, ServerTimestamp>> {
            match &self.info_collections {
                Ok(collections) => Ok(collections.clone()),
                Err(err) => Err(error::unexpected("Can't fetch info/collections")),
            }
        }

        fn fetch_meta_global(&self) -> error::Result<BsoRecord<MetaGlobalRecord>> {
            match &self.meta_global {
                Ok(global) => Ok(global.clone()),
                // TODO: Special handling for 404s, we want to ensure we handle
                // missing keys and other server errors correctly.
                Err(err) => Err(error::unexpected("Can't fetch meta/global")),
            }
        }

        fn fetch_crypto_keys(&self) -> error::Result<BsoRecord<EncryptedPayload>> {
            match &self.crypto_keys {
                Ok(keys) => Ok(keys.clone()),
                // TODO: Same as above, for 404s.
                Err(err) => Err(error::unexpected("Can't fetch crypto/keys")),
            }
        }

        fn wipe_storage(&self) -> error::Result<()> {
            Ok(())
        }
    }

    struct InMemoryScratchpad {
        config: InfoConfiguration,
        global_record: Option<BsoRecord<MetaGlobalRecord>>,
        collection_keys: Option<CollectionKeys>,
    }

    impl Scratchpad for InMemoryScratchpad {
        fn config(&self) -> &InfoConfiguration {
            return &self.config;
        }

        fn set_config(&mut self, config: InfoConfiguration) {
            self.config = config
        }

        #[inline]
        fn global(&self) -> Option<&BsoRecord<MetaGlobalRecord>> {
            self.global_record.as_ref()
        }

        fn set_global(&mut self, global: Option<BsoRecord<MetaGlobalRecord>>) {
            self.global_record = global;
        }

        #[inline]
        fn keys(&self) -> Option<&CollectionKeys> {
            self.collection_keys.as_ref()
        }

        fn set_keys(&mut self, keys: Option<CollectionKeys>) {
            self.collection_keys = keys;
        }

        fn add_engine_state_change(&mut self, change: EngineStateChange) {
            // ...
        }

        fn add_engine_state_changes_from_keys(&mut self, keys: Option<&CollectionKeys>) {
            // ...
        }

        fn save(&mut self) {
            // ...
        }
    }

    #[test]
    fn test_state_machine() {
        let root_key = KeyBundle::new_random().unwrap();
        let keys = CollectionKeys {
            modified: 123.4.into(),
            default: KeyBundle::new_random().unwrap(),
            collections: HashMap::new(),
        };
        let client = Rc::new(InMemoryClient {
            info_configuration: Ok(InfoConfiguration::default()),
            info_collections: Ok(vec![("meta", 123.456), ("crypto", 145.0)]
                .iter()
                .cloned()
                .map(|(key, value)| (key.to_owned(), value.into()))
                .collect()),
            meta_global: Ok(MetaGlobalRecord {
                sync_id: "syncIDAAAAAA".to_owned(),
                storage_version: 5usize,
                engines: vec![
                    (
                        "bookmarks",
                        MetaGlobalEngine {
                            version: 1usize,
                            sync_id: "syncIDBBBBBB".to_owned(),
                        },
                    ),
                ].iter()
                    .cloned()
                    .map(|(key, value)| (key.to_owned(), value.into()))
                    .collect(),
                declined: vec![],
            }.into()),
            crypto_keys: keys.to_encrypted_bso(&root_key),
        });
        let mut scratchpad = InMemoryScratchpad {
            config: InfoConfiguration::default(),
            global_record: None,
            collection_keys: None,
        };

        let mut state_machine = SyncStateMachine::new(client, &mut scratchpad, &root_key);
        assert!(
            state_machine.to_ready().is_ok(),
            "Should drive state machine to ready"
        );
        assert_eq!(
            &state_machine.label_sequence,
            &vec![
                "InitialWithLiveToken",
                "InitialWithLiveTokenAndInfo",
                "NeedsFreshMetaGlobal",
                "ResolveMetaGlobal",
                "HasMetaGlobal",
                "NeedsFreshCryptoKeys",
                "Ready",
            ],
            "Should cycle through all states"
        );
    }
}

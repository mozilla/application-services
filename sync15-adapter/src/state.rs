/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::{HashMap, HashSet};
use std::mem;

use bso_record::BsoRecord;
use client::SetupStorageClient;
use collection_keys::CollectionKeys;
use error::{self, ErrorKind};
use key_bundle::KeyBundle;
use record_types::{MetaGlobalEngine, MetaGlobalRecord};
use request::InfoConfiguration;
use util::{random_guid, ServerTimestamp, SERVER_EPOCH};

use self::SetupState::*;

const STORAGE_VERSION: usize = 5;

lazy_static! {
    /// Maps names to storage versions for engines to include in a fresh
    /// `meta/global` record.
    static ref DEFAULT_ENGINES: Vec<(&'static str, usize)> = vec![
        ("passwords", 1),
        // Unsupported engines.
        ("clients", 1),
        ("addons", 1),
        ("addresses", 1),
        ("bookmarks", 2),
        ("creditcards", 1),
        ("forms", 1),
        ("history", 1),
        ("prefs", 2),
        ("tabs", 1),
    ];

    // Declined engines to include in a fresh `meta/global` record.
    static ref DEFAULT_DECLINED: Vec<&'static str> = vec![];
}

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

/// Creates a fresh `meta/global` record, using the default engine selections,
/// and declined engines from the previous record.
fn new_global_from_previous(
    previous_global: Option<BsoRecord<MetaGlobalRecord>>,
) -> error::Result<MetaGlobalRecord> {
    let sync_id = random_guid()?;
    let mut engines: HashMap<String, _> = HashMap::new();
    for (name, version) in DEFAULT_ENGINES.iter() {
        let sync_id = random_guid()?;
        engines.insert(
            name.to_string(),
            MetaGlobalEngine {
                version: *version,
                sync_id,
            },
        );
    }
    Ok(MetaGlobalRecord {
        sync_id,
        storage_version: STORAGE_VERSION,
        engines,
        declined: previous_global
            .as_ref()
            .map(|global| global.declined.clone())
            .unwrap_or_else(|| {
                DEFAULT_DECLINED
                    .iter()
                    .map(|name| name.to_string())
                    .collect()
            }),
    })
}

pub struct SetupStateMachine<'c, 's, 'k> {
    client: &'c SetupStorageClient,
    state: &'s mut GlobalState,
    root_key: &'k KeyBundle,
    sequence: Vec<mem::Discriminant<SetupState>>,
    engine_state_changes: Vec<EngineStateChange>,
}

impl<'c, 's, 'k> SetupStateMachine<'c, 's, 'k> {
    pub fn new(
        client: &'c SetupStorageClient,
        state: &'s mut GlobalState,
        root_key: &'k KeyBundle,
    ) -> SetupStateMachine<'c, 's, 'k> {
        SetupStateMachine {
            client,
            state,
            root_key,
            sequence: Vec::new(),
            engine_state_changes: Vec::new(),
        }
    }

    /// Returns a set of all engine names that should be reset locally.
    pub fn engines_that_need_reset(&self) -> HashSet<String> {
        let all_engines = self.state
            .global
            .as_ref()
            .map(|global| {
                global
                    .engines
                    .keys()
                    .map(|key| key.to_string())
                    .collect::<HashSet<String>>()
            })
            .unwrap_or_default();
        let mut engines_to_reset = HashSet::new();
        for change in &self.engine_state_changes {
            match change {
                EngineStateChange::Reset(name) => {
                    engines_to_reset.insert(name.to_string());
                }
                EngineStateChange::ResetAll { except } => {
                    for engine in all_engines.difference(except) {
                        engines_to_reset.insert(engine.to_string());
                    }
                }
                _ => {}
            }
        }
        engines_to_reset
    }

    fn advance(&mut self, from: SetupState) -> error::Result<SetupState> {
        match from {
            // Fetch `info/configuration` with current server limits, and
            // `info/collections` with collection last modified times.
            InitialWithLiveToken => {
                let config = self.client.fetch_info_configuration().unwrap_or_default();
                self.state.config = config;

                let collections = self.client.fetch_info_collections()?;
                self.state.collections = collections;
                Ok(InitialWithLiveTokenAndInfo)
            }

            // Compare local and remote `meta/global` timestamps to determine
            // if our locally cached `meta/global` is up-to-date.
            InitialWithLiveTokenAndInfo => {
                let action = {
                    let local = self.state.global.as_ref().map(|global| &global.modified);
                    let remote = self.state.collections.get("meta");
                    FetchAction::from_modified(local, remote)
                };
                Ok(match action {
                    // Hooray, we don't need to fetch `meta/global`. Skip to
                    // the next state.
                    FetchAction::Skip => HasMetaGlobal,
                    // Our `meta/global` is out of date, or isn't cached
                    // locally, so we need to fetch it from the server.
                    FetchAction::Fetch => {
                        let previous_global = mem::replace(&mut self.state.global, None);
                        NeedsFreshMetaGlobal { previous_global }
                    }
                    // We have a `meta/global` record in our cache, but not on
                    // the server. This likely means we're the first client to
                    // sync after a node reassignment. Invalidate our cached
                    // `meta/global` and `crypto/keys`, and try to fetch
                    // `meta/global` from the server anyway. If another client
                    // wins the race, we'll fetch its `meta/global`; if not,
                    // we'll fail and upload our own.
                    FetchAction::InvalidateThenUpload => {
                        let previous_global = mem::replace(&mut self.state.global, None);
                        self.state.keys = None;
                        NeedsFreshMetaGlobal { previous_global }
                    }
                })
            }

            // Fetch `meta/global` from the server.
            NeedsFreshMetaGlobal { previous_global } => match self.client.fetch_meta_global() {
                Ok(new_global) => Ok(ResolveMetaGlobal {
                    previous_global,
                    new_global,
                }),
                Err(err) => match err.kind() {
                    ErrorKind::NoMetaGlobal { .. } => Ok(FreshStartRequired { previous_global }),
                    _ => Err(err),
                },
            },

            // Reconcile the server's `meta/global` with our locally cached
            // `meta/global`, if any.
            ResolveMetaGlobal {
                previous_global,
                new_global,
            } => {
                // If the server has a newer storage version, we can't
                // sync until our client is updated.
                if &new_global.payload.storage_version > &STORAGE_VERSION {
                    return Err(ErrorKind::ClientUpgradeRequired.into());
                }

                // If the server has an older storage version, wipe and
                // reupload.
                if &new_global.payload.storage_version < &STORAGE_VERSION {
                    return Ok(FreshStartRequired { previous_global });
                }

                match &previous_global {
                    Some(previous_global) => {
                        if &previous_global.sync_id != &new_global.sync_id {
                            self.state.keys = None;
                            self.engine_state_changes.push(EngineStateChange::ResetAll {
                                except: HashSet::new(),
                            });
                        } else {
                            let previous_engine_names =
                                previous_global.engines.keys().collect::<HashSet<&String>>();
                            let new_engine_names =
                                new_global.engines.keys().collect::<HashSet<&String>>();

                            // Disable any local engines that aren't mentioned
                            // in the new `meta/global`.
                            for name in previous_engine_names.difference(&new_engine_names) {
                                self.engine_state_changes
                                    .push(EngineStateChange::Disable(name.to_string()));
                            }

                            // Enable any new engines that aren't mentioned in
                            // the locally cached `meta/global`.
                            for name in new_engine_names.difference(&previous_engine_names) {
                                self.engine_state_changes
                                    .push(EngineStateChange::Enable(name.to_string()));
                            }

                            // Reset engines with sync ID changes.
                            for name in new_engine_names.intersection(&previous_engine_names) {
                                let previous_engine = previous_global.engines.get(*name).unwrap();
                                let new_engine = new_global.engines.get(*name).unwrap();
                                if previous_engine.sync_id != new_engine.sync_id {
                                    self.engine_state_changes
                                        .push(EngineStateChange::Reset(name.to_string()));
                                }
                            }
                        }
                    }
                    None => {
                        self.state.keys = None;
                        self.engine_state_changes.push(EngineStateChange::ResetAll {
                            except: HashSet::new(),
                        });
                    }
                }
                self.state.global = Some(new_global);
                Ok(HasMetaGlobal)
            }

            // Check if our locally cached `crypto/keys` collection is
            // up-to-date.
            HasMetaGlobal => {
                // TODO(lina): Check if we've enabled or disabled any engines
                // locally, and update `m/g` to reflect that.
                let action = {
                    let local = self.state.keys.as_ref().map(|keys| &keys.timestamp);
                    let remote = self.state.collections.get("crypto");
                    FetchAction::from_modified(local, remote)
                };
                Ok(match action {
                    // If `crypto/keys` is up-to-date, we're ready to go!
                    FetchAction::Skip => Ready,
                    // We need to fetch and cache new keys.
                    FetchAction::Fetch => NeedsFreshCryptoKeys,
                    // We need to invalidate our locally cached `crypto/keys`,
                    // then try to fetch new keys, and reupload if fetching
                    // fails.
                    FetchAction::InvalidateThenUpload => {
                        self.state.keys = None;
                        NeedsFreshCryptoKeys
                    }
                })
            }

            NeedsFreshCryptoKeys => {
                match self.client.fetch_crypto_keys() {
                    Ok(encrypted_bso) => {
                        let fresh_keys =
                            CollectionKeys::from_encrypted_bso(encrypted_bso, self.root_key)?;
                        match &self.state.keys {
                            Some(stale_keys) => {
                                if fresh_keys.default == stale_keys.default {
                                    // The default bundle is the same, so only reset
                                    // engines with different collection-specific keys.
                                    for (collection, key_bundle) in &stale_keys.collections {
                                        if key_bundle != fresh_keys.key_for_collection(collection) {
                                            self.engine_state_changes.push(
                                                EngineStateChange::Reset(collection.to_string()),
                                            );
                                        }
                                    }
                                    for (collection, key_bundle) in &fresh_keys.collections {
                                        if key_bundle != stale_keys.key_for_collection(collection) {
                                            self.engine_state_changes.push(
                                                EngineStateChange::Reset(collection.to_string()),
                                            );
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
                                    self.engine_state_changes
                                        .push(EngineStateChange::ResetAll { except });
                                }
                            }
                            None => self.engine_state_changes.push(EngineStateChange::ResetAll {
                                except: HashSet::new(),
                            }),
                        }
                        self.state.keys = Some(fresh_keys);
                        Ok(Ready)
                    }
                    Err(err) => match err.kind() {
                        // If the server doesn't have a `crypto/keys`, start over
                        // and reupload our `meta/global` and `crypto/keys`.
                        ErrorKind::NoCryptoKeys { .. } => {
                            let previous_global = mem::replace(&mut self.state.global, None);
                            Ok(FreshStartRequired { previous_global })
                        }
                        _ => Err(err),
                    },
                }
            }

            Ready => Err(ErrorKind::UnexpectedSetupState.into()),

            FreshStartRequired { previous_global } => {
                // Wipe the server.
                self.client.wipe_all()?;
                self.state.global = None;
                self.state.keys = None;
                self.engine_state_changes.push(EngineStateChange::ResetAll {
                    except: HashSet::new(),
                });

                // Upload a fresh `meta/global`...
                let new_global = BsoRecord {
                    id: "global".into(),
                    collection: "meta".into(),
                    modified: 0.0.into(), // Doesn't matter.
                    sortindex: None,
                    ttl: None,
                    payload: new_global_from_previous(previous_global)?,
                };
                self.client.put_meta_global(&new_global)?;

                // ...And a fresh `crypto/keys`. Note that we'll update the
                // global state when we go around the state machine again,
                // not here.
                let new_keys = CollectionKeys::new_random()?.to_encrypted_bso(&self.root_key)?;
                self.client.put_crypto_keys(&new_keys)?;

                Ok(InitialWithLiveToken)
            }
        }
    }

    /// Runs through the state machine to the ready state.
    pub fn to_ready(&mut self) -> error::Result<()> {
        let mut state = InitialWithLiveToken;
        loop {
            match &state {
                Ready => {
                    self.sequence.push(mem::discriminant(&state));
                    break;
                }
                // If we already started over once before, we're likely in a
                // cycle, and should try again later. Like the iOS state
                // machine, other cycles aren't a problem; we'll cycle through
                // earlier states if we need to reupload `meta/global` or
                // `crypto/keys`.
                FreshStartRequired { .. } if self.sequence.contains(&mem::discriminant(&state)) => {
                    return Err(ErrorKind::SetupStateCycleError.into());
                }
                _ => {
                    self.sequence.push(mem::discriminant(&state));
                    state = self.advance(state)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
enum SetupState {
    InitialWithLiveToken,
    InitialWithLiveTokenAndInfo,
    NeedsFreshMetaGlobal {
        previous_global: Option<BsoRecord<MetaGlobalRecord>>,
    },
    HasMetaGlobal,
    ResolveMetaGlobal {
        previous_global: Option<BsoRecord<MetaGlobalRecord>>,
        new_global: BsoRecord<MetaGlobalRecord>,
    },
    NeedsFreshCryptoKeys,
    Ready,
    FreshStartRequired {
        previous_global: Option<BsoRecord<MetaGlobalRecord>>,
    },
}

/// Whether we should skip fetching `meta/global` or `crypto/keys` from the
/// server because our locally cached copy is up-to-date, fetch a fresh copy
/// from the server, or invalidate our locally cached state, then upload a
/// fresh record.
enum FetchAction {
    Skip,
    Fetch,
    InvalidateThenUpload,
}

impl FetchAction {
    fn from_modified(
        local: Option<&ServerTimestamp>,
        remote: Option<&ServerTimestamp>,
    ) -> FetchAction {
        match (local, remote) {
            (Some(local), Some(remote)) => {
                if *local >= *remote {
                    FetchAction::Skip
                } else {
                    FetchAction::Fetch
                }
            }
            (Some(_), None) => FetchAction::InvalidateThenUpload,
            _ => FetchAction::Fetch,
        }
    }
}

/// Flags an engine for enablement or disablement.
pub enum EngineStateChange {
    ResetAll { except: HashSet<String> },
    Enable(String),
    Disable(String),
    Reset(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest;

    use bso_record::{BsoRecord, EncryptedBso, EncryptedPayload};

    struct InMemoryClient {
        info_configuration: error::Result<InfoConfiguration>,
        info_collections: error::Result<HashMap<String, ServerTimestamp>>,
        meta_global: error::Result<BsoRecord<MetaGlobalRecord>>,
        crypto_keys: error::Result<BsoRecord<EncryptedPayload>>,
    }

    impl SetupStorageClient for InMemoryClient {
        fn fetch_info_configuration(&self) -> error::Result<InfoConfiguration> {
            match &self.info_configuration {
                Ok(config) => Ok(config.clone()),
                Err(_) => Err(ErrorKind::StorageHttpError {
                    code: reqwest::StatusCode::InternalServerError,
                    route: "info/configuration".to_string(),
                }.into()),
            }
        }

        fn fetch_info_collections(&self) -> error::Result<HashMap<String, ServerTimestamp>> {
            match &self.info_collections {
                Ok(collections) => Ok(collections.clone()),
                Err(_) => Err(ErrorKind::StorageHttpError {
                    code: reqwest::StatusCode::InternalServerError,
                    route: "info/collections".to_string(),
                }.into()),
            }
        }

        fn fetch_meta_global(&self) -> error::Result<BsoRecord<MetaGlobalRecord>> {
            match &self.meta_global {
                Ok(global) => Ok(global.clone()),
                // TODO(lina): Special handling for 404s, we want to ensure we
                // handle missing keys and other server errors correctly.
                Err(_) => Err(ErrorKind::StorageHttpError {
                    code: reqwest::StatusCode::InternalServerError,
                    route: "meta/global".to_string(),
                }.into()),
            }
        }

        fn put_meta_global(&self, global: &BsoRecord<MetaGlobalRecord>) -> error::Result<()> {
            Err(ErrorKind::StorageHttpError {
                code: reqwest::StatusCode::InternalServerError,
                route: "meta/global".to_string(),
            }.into())
        }

        fn fetch_crypto_keys(&self) -> error::Result<BsoRecord<EncryptedPayload>> {
            match &self.crypto_keys {
                Ok(keys) => Ok(keys.clone()),
                // TODO(lina): Same as above, for 404s.
                Err(_) => Err(ErrorKind::StorageHttpError {
                    code: reqwest::StatusCode::InternalServerError,
                    route: "crypto/keys".to_string(),
                }.into()),
            }
        }

        fn put_crypto_keys(&self, keys: &EncryptedBso) -> error::Result<()> {
            Err(ErrorKind::StorageHttpError {
                code: reqwest::StatusCode::InternalServerError,
                route: "crypto/keys".to_string(),
            }.into())
        }

        fn wipe_all(&self) -> error::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_state_machine() {
        let root_key = KeyBundle::new_random().unwrap();
        let keys = CollectionKeys {
            timestamp: 123.4.into(),
            default: KeyBundle::new_random().unwrap(),
            collections: HashMap::new(),
        };
        let client = InMemoryClient {
            info_configuration: Ok(InfoConfiguration::default()),
            info_collections: Ok(vec![("meta", 123.456), ("crypto", 145.0)]
                .iter()
                .cloned()
                .map(|(key, value)| (key.to_owned(), value.into()))
                .collect()),
            meta_global: Ok(BsoRecord {
                id: "global".into(),
                modified: ServerTimestamp(999.0),
                collection: "meta".into(),
                sortindex: None,
                ttl: None,
                payload: MetaGlobalRecord {
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
                },
            }),
            crypto_keys: keys.to_encrypted_bso(&root_key),
        };

        let mut state = GlobalState::default();
        let sequence = {
            let mut state_machine = SetupStateMachine::new(&client, &mut state, &root_key);
            assert!(
                state_machine.to_ready().is_ok(),
                "Should drive state machine to ready"
            );
            state_machine.sequence
        };
        assert_eq!(
            sequence,
            vec![
                InitialWithLiveToken,
                InitialWithLiveTokenAndInfo,
                NeedsFreshMetaGlobal {
                    previous_global: None,
                },
                ResolveMetaGlobal {
                    previous_global: None,
                    new_global: state.global.unwrap(),
                },
                HasMetaGlobal,
                NeedsFreshCryptoKeys,
                Ready,
            ].into_iter()
                .map(|state| mem::discriminant(&state))
                .collect::<Vec<mem::Discriminant<_>>>(),
            "Should cycle through all states"
        );
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::mem;

use crate::client::{SetupStorageClient, Sync15ClientResponse};
use crate::collection_keys::CollectionKeys;
use crate::error::{self, ErrorKind};
use crate::key_bundle::KeyBundle;
use crate::record_types::{MetaGlobalEngine, MetaGlobalRecord};
use crate::request::{InfoCollections, InfoConfiguration};
use crate::util::{random_guid, ServerTimestamp};
use interrupt::Interruptee;
use lazy_static::lazy_static;
use serde_derive::*;

use self::SetupState::*;

const STORAGE_VERSION: usize = 5;

lazy_static! {
    /// Maps names to storage versions for engines to include in a fresh
    /// `meta/global` record. We include engines that we don't implement
    /// because they'll be disabled on other clients if we omit them
    /// (bug 1479929).
    static ref DEFAULT_ENGINES: Vec<(&'static str, usize)> = vec![
        ("passwords", 1),
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

/// State that we require the app to persist to storage for us.
/// It's a little unfortunate we need this, because it's only tracking
/// "declined engines", and even then, only needed in practice when there's
/// no meta/global so we need to create one. It's extra unfortunate because we
/// want to move away from "globally declined" engines anyway, moving towards
/// allowing engines to be enabled or disabled per client rather than globally.
///
/// Apps are expected to treat this as opaque, so we support serializing it.
/// Note that this structure is *not* used to *change* the declined engines
/// list - that will be done in the future, but the API exposed for that
/// purpose will also take a mutable PersistedGlobalState.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "schema_version")]
pub enum PersistedGlobalState {
    /// V1 was when we persisted the entire GlobalState, keys and all!

    /// V2 is just tracking the globally declined list.
    /// None means "I've no idea" and theoretically should only happen on the
    /// very first sync for an app.
    V2 { declined: Option<Vec<String>> },
}

impl Default for PersistedGlobalState {
    #[inline]
    fn default() -> PersistedGlobalState {
        PersistedGlobalState::V2 { declined: None }
    }
}

/// Holds global Sync state, including server upload limits, and the
/// last-fetched collection modified times, `meta/global` record, and
/// the default encryption key.
#[derive(Debug, Clone)]
pub struct GlobalState {
    pub config: InfoConfiguration,
    pub collections: InfoCollections,
    pub global: MetaGlobalRecord,
    pub global_timestamp: ServerTimestamp,
    pub keys: CollectionKeys,
}

/// Creates a fresh `meta/global` record, using the default engine selections,
/// and declined engines from our PersistedGlobalState.
fn new_global(pgs: &PersistedGlobalState) -> error::Result<MetaGlobalRecord> {
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
    // We only need our PersistedGlobalState to fill out a new meta/global - if
    // we previously saw a meta/global then we would have updated it with what
    // it was at the time.
    let declined = match pgs {
        PersistedGlobalState::V2 { declined: Some(d) } => d.clone(),
        _ => {
            log::warn!("New meta/global without local app state - the list of declined engines is being reset");
            DEFAULT_DECLINED.iter().map(ToString::to_string).collect()
        }
    };

    Ok(MetaGlobalRecord {
        sync_id,
        storage_version: STORAGE_VERSION,
        engines,
        declined,
    })
}

pub struct SetupStateMachine<'a> {
    client: &'a dyn SetupStorageClient,
    root_key: &'a KeyBundle,
    pgs: &'a mut PersistedGlobalState,
    // `allowed_states` is designed so that we can arrange for the concept of
    // a "fast" sync - so we decline to advance if we need to setup from scratch.
    // The idea is that if we need to sync before going to sleep we should do
    // it as fast as possible. However, in practice this isn't going to do
    // what we expect - a "fast sync" that finds lots to do is almost certainly
    // going to take longer than a "full sync" that finds nothing to do.
    // We should almost certainly remove this and instead allow for a "time
    // budget", after which we get interrupted. Later...
    allowed_states: Vec<&'static str>,
    sequence: Vec<&'static str>,
    interruptee: &'a dyn Interruptee,
}

impl<'a> SetupStateMachine<'a> {
    /// Creates a state machine for a "classic" Sync 1.5 client that supports
    /// all states, including uploading a fresh `meta/global` and `crypto/keys`
    /// after a node reassignment.
    pub fn for_full_sync(
        client: &'a dyn SetupStorageClient,
        root_key: &'a KeyBundle,
        pgs: &'a mut PersistedGlobalState,
        interruptee: &'a dyn Interruptee,
    ) -> SetupStateMachine<'a> {
        SetupStateMachine::with_allowed_states(
            client,
            root_key,
            pgs,
            interruptee,
            vec![
                "Initial",
                "InitialWithConfig",
                "InitialWithInfo",
                "InitialWithMetaGlobal",
                "Ready",
                "FreshStartRequired",
                "WithPreviousState",
            ],
        )
    }

    /// Creates a state machine for a fast sync, which only uses locally
    /// cached global state, and bails if `meta/global` or `crypto/keys`
    /// are missing or out-of-date. This is useful in cases where it's
    /// important to get to ready as quickly as possible, like syncing before
    /// sleep, or when conserving time or battery life.
    pub fn for_fast_sync(
        client: &'a dyn SetupStorageClient,
        root_key: &'a KeyBundle,
        pgs: &'a mut PersistedGlobalState,
        interruptee: &'a dyn Interruptee,
    ) -> SetupStateMachine<'a> {
        SetupStateMachine::with_allowed_states(
            client,
            root_key,
            pgs,
            interruptee,
            vec!["Ready", "WithPreviousState"],
        )
    }

    /// Creates a state machine for a read-only sync, where the client can't
    /// upload `meta/global` or `crypto/keys`. Useful for clients that only
    /// sync specific collections, like Lockbox.
    pub fn for_readonly_sync(
        client: &'a dyn SetupStorageClient,
        root_key: &'a KeyBundle,
        pgs: &'a mut PersistedGlobalState,
        interruptee: &'a dyn Interruptee,
    ) -> SetupStateMachine<'a> {
        SetupStateMachine::with_allowed_states(
            client,
            root_key,
            pgs,
            interruptee,
            // We don't allow a FreshStart in a read-only sync.
            vec![
                "Initial",
                "InitialWithConfig",
                "InitialWithInfo",
                "InitialWithMetaGlobal",
                "Ready",
                "WithPreviousState",
            ],
        )
    }

    fn with_allowed_states(
        client: &'a dyn SetupStorageClient,
        root_key: &'a KeyBundle,
        pgs: &'a mut PersistedGlobalState,
        interruptee: &'a dyn Interruptee,
        allowed_states: Vec<&'static str>,
    ) -> SetupStateMachine<'a> {
        SetupStateMachine {
            client,
            root_key,
            pgs,
            sequence: Vec::new(),
            allowed_states,
            interruptee,
        }
    }

    fn advance(&mut self, from: SetupState) -> error::Result<SetupState> {
        match from {
            // Fetch `info/configuration` with current server limits, and
            // `info/collections` with collection last modified times.
            Initial => {
                let config = match self.client.fetch_info_configuration()? {
                    Sync15ClientResponse::Success { record, .. } => record,
                    Sync15ClientResponse::NotFound { .. } => InfoConfiguration::default(),
                    other => return Err(other.create_storage_error().into()),
                };
                Ok(InitialWithConfig { config })
            }

            // XXX - we could consider combining these Initial* states, because we don't
            // attempt to support filling in "missing" global state - *any* 404 in them
            // means `FreshStart`.
            // IOW, in all cases, they either `Err()`, move to `FreshStartRequired`, or
            // advance to a specific next state.
            InitialWithConfig { config } => {
                match self.client.fetch_info_collections()? {
                    Sync15ClientResponse::Success {
                        record: collections,
                        ..
                    } => Ok(InitialWithInfo {
                        config,
                        collections,
                    }),
                    // If the server doesn't have a `crypto/keys`, start over
                    // and reupload our `meta/global` and `crypto/keys`.
                    Sync15ClientResponse::NotFound { .. } => Ok(FreshStartRequired { config }),
                    other => Err(other.create_storage_error().into()),
                }
            }

            InitialWithInfo {
                config,
                collections,
            } => match self.client.fetch_meta_global()? {
                Sync15ClientResponse::Success {
                    record: global,
                    last_modified: global_timestamp,
                    ..
                } => {
                    // If the server has a newer storage version, we can't
                    // sync until our client is updated.
                    if global.storage_version > STORAGE_VERSION {
                        return Err(ErrorKind::ClientUpgradeRequired.into());
                    }

                    // If the server has an older storage version, wipe and
                    // reupload.
                    if global.storage_version < STORAGE_VERSION {
                        Ok(FreshStartRequired { config })
                    } else {
                        // TODO: Here would be a good place to check if we've enabled
                        // or disabled any engines locally, and update `m/g` to reflect
                        // that.
                        Ok(InitialWithMetaGlobal {
                            config,
                            collections,
                            global,
                            global_timestamp,
                        })
                    }
                }
                Sync15ClientResponse::NotFound { .. } => Ok(FreshStartRequired { config }),
                other => Err(other.create_storage_error().into()),
            },

            InitialWithMetaGlobal {
                config,
                collections,
                global,
                global_timestamp,
            } => {
                // Update our PersistedGlobalState with the mega/global we just read.
                mem::replace(
                    self.pgs,
                    PersistedGlobalState::V2 {
                        declined: Some(global.declined.clone()),
                    },
                );
                // Now try and get keys etc - if we fresh-start we'll re-use declined.
                match self.client.fetch_crypto_keys()? {
                    Sync15ClientResponse::Success {
                        record,
                        last_modified,
                        ..
                    } => {
                        // Note that collection/keys is itself a bso, so the
                        // json body also carries the timestamp. If they aren't
                        // identical something has screwed up and we should die.
                        assert_eq!(last_modified, record.modified);
                        let new_keys = CollectionKeys::from_encrypted_bso(record, self.root_key)?;
                        let state = GlobalState {
                            config,
                            collections,
                            global,
                            global_timestamp,
                            keys: new_keys,
                        };
                        Ok(Ready { state })
                    }
                    // If the server doesn't have a `crypto/keys`, start over
                    // and reupload our `meta/global` and `crypto/keys`.
                    Sync15ClientResponse::NotFound { .. } => Ok(FreshStartRequired { config }),
                    other => Err(other.create_storage_error().into()),
                }
            }

            // We've got old state that's likely to be OK.
            // We keep things simple here - if there's evidence of a new/missing
            // meta/global or new/missing keys we just restart from scratch.
            WithPreviousState { old_state } => match self.client.fetch_info_collections()? {
                Sync15ClientResponse::Success {
                    record: collections,
                    ..
                } => Ok(
                    if is_fresh(old_state.global_timestamp, &collections, "meta")
                        && is_fresh(old_state.keys.timestamp, &collections, "crypto")
                    {
                        Ready {
                            state: GlobalState {
                                collections,
                                ..old_state
                            },
                        }
                    } else {
                        InitialWithConfig {
                            config: old_state.config,
                        }
                    },
                ),
                _ => Ok(InitialWithConfig {
                    config: old_state.config,
                }),
            },

            Ready { state } => Ok(Ready { state }),

            FreshStartRequired { config } => {
                // Wipe the server.
                self.client.wipe_all_remote()?;

                // Upload a fresh `meta/global`...
                let new_global = new_global(self.pgs)?;
                self.client
                    .put_meta_global(ServerTimestamp::default(), &new_global)?;

                // ...And a fresh `crypto/keys`.
                let new_keys = CollectionKeys::new_random()?.to_encrypted_bso(&self.root_key)?;
                self.client
                    .put_crypto_keys(ServerTimestamp::default(), &new_keys)?;

                // TODO(lina): Can we pass along server timestamps from the PUTs
                // above, and avoid re-fetching the `m/g` and `c/k` we just
                // uploaded?
                // OTOH(mark): restarting the state machine keeps life simple and rare.
                Ok(InitialWithConfig { config })
            }
        }
    }

    /// Runs through the state machine to the ready state.
    pub fn run_to_ready(&mut self, state: Option<GlobalState>) -> error::Result<GlobalState> {
        let mut s = match state {
            Some(old_state) => WithPreviousState { old_state },
            None => Initial,
        };
        loop {
            self.interruptee.err_if_interrupted()?;
            let label = &s.label();
            log::trace!("global state: {:?}", label);
            match s {
                Ready { state } => {
                    self.sequence.push(label);
                    return Ok(state);
                }
                // If we already started over once before, we're likely in a
                // cycle, and should try again later. Intermediate states
                // aren't a problem, just the initial ones.
                FreshStartRequired { .. } | WithPreviousState { .. } | Initial => {
                    if self.sequence.contains(&label) {
                        // Is this really the correct error?
                        return Err(ErrorKind::SetupRace.into());
                    }
                }
                _ => {
                    if !self.allowed_states.contains(&label) {
                        return Err(ErrorKind::SetupRequired.into());
                    }
                }
            };
            self.sequence.push(label);
            s = self.advance(s)?;
        }
    }
}

/// States in the remote setup process.
/// TODO(lina): Add link once #56 is merged.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum SetupState {
    // These "Initial" states are only ever used when starting from scratch.
    Initial,
    InitialWithConfig {
        config: InfoConfiguration,
    },
    InitialWithInfo {
        config: InfoConfiguration,
        collections: InfoCollections,
    },
    InitialWithMetaGlobal {
        config: InfoConfiguration,
        collections: InfoCollections,
        global: MetaGlobalRecord,
        global_timestamp: ServerTimestamp,
    },
    WithPreviousState {
        old_state: GlobalState,
    },
    Ready {
        state: GlobalState,
    },
    FreshStartRequired {
        config: InfoConfiguration,
    },
}

impl SetupState {
    fn label(&self) -> &'static str {
        match self {
            Initial { .. } => "Initial",
            InitialWithConfig { .. } => "InitialWithConfig",
            InitialWithInfo { .. } => "InitialWithInfo",
            InitialWithMetaGlobal { .. } => "InitialWithMetaGlobal",
            Ready { .. } => "Ready",
            WithPreviousState { .. } => "WithPreviousState",
            FreshStartRequired { .. } => "FreshStartRequired",
        }
    }
}

/// Whether we should skip fetching an item.
fn is_fresh(local: ServerTimestamp, collections: &InfoCollections, key: &str) -> bool {
    collections.get(key).map_or(false, |ts| local >= *ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::bso_record::{BsoRecord, EncryptedBso, EncryptedPayload, Payload};
    use crate::record_types::CryptoKeysRecord;
    use interrupt::NeverInterrupts;

    struct InMemoryClient {
        info_configuration: error::Result<Sync15ClientResponse<InfoConfiguration>>,
        info_collections: error::Result<Sync15ClientResponse<InfoCollections>>,
        meta_global: error::Result<Sync15ClientResponse<MetaGlobalRecord>>,
        crypto_keys: error::Result<Sync15ClientResponse<BsoRecord<EncryptedPayload>>>,
    }

    impl SetupStorageClient for InMemoryClient {
        fn fetch_info_configuration(
            &self,
        ) -> error::Result<Sync15ClientResponse<InfoConfiguration>> {
            match &self.info_configuration {
                Ok(client_response) => Ok(client_response.clone()),
                Err(_) => Ok(Sync15ClientResponse::ServerError {
                    status: 500,
                    route: "test/path".into(),
                }),
            }
        }

        fn fetch_info_collections(&self) -> error::Result<Sync15ClientResponse<InfoCollections>> {
            match &self.info_collections {
                Ok(collections) => Ok(collections.clone()),
                Err(_) => Ok(Sync15ClientResponse::ServerError {
                    status: 500,
                    route: "test/path".into(),
                }),
            }
        }

        fn fetch_meta_global(&self) -> error::Result<Sync15ClientResponse<MetaGlobalRecord>> {
            match &self.meta_global {
                Ok(global) => Ok(global.clone()),
                // TODO(lina): Special handling for 404s, we want to ensure we
                // handle missing keys and other server errors correctly.
                Err(_) => Ok(Sync15ClientResponse::ServerError {
                    status: 500,
                    route: "test/path".into(),
                }),
            }
        }

        fn put_meta_global(
            &self,
            xius: ServerTimestamp,
            _global: &MetaGlobalRecord,
        ) -> error::Result<()> {
            assert_eq!(xius, ServerTimestamp(999.9));
            Err(ErrorKind::StorageHttpError {
                code: 500,
                route: "meta/global".to_string(),
            }
            .into())
        }

        fn fetch_crypto_keys(&self) -> error::Result<Sync15ClientResponse<EncryptedBso>> {
            match &self.crypto_keys {
                Ok(keys) => Ok(keys.clone()),
                // TODO(lina): Same as above, for 404s.
                Err(_) => Ok(Sync15ClientResponse::ServerError {
                    status: 500,
                    route: "test/path".into(),
                }),
            }
        }

        fn put_crypto_keys(
            &self,
            xius: ServerTimestamp,
            _keys: &EncryptedBso,
        ) -> error::Result<()> {
            assert_eq!(xius, ServerTimestamp(888.8));
            Err(ErrorKind::StorageHttpError {
                code: 500,
                route: "crypto/keys".to_string(),
            }
            .into())
        }

        fn wipe_all_remote(&self) -> error::Result<()> {
            Ok(())
        }
    }

    fn mocked_success_ts<T>(t: T, ts: f64) -> error::Result<Sync15ClientResponse<T>> {
        Ok(Sync15ClientResponse::Success {
            record: t,
            last_modified: ServerTimestamp(ts),
            route: "test/path".into(),
        })
    }

    fn mocked_success<T>(t: T) -> error::Result<Sync15ClientResponse<T>> {
        mocked_success_ts(t, 0.0)
    }

    // for tests, we want a BSO with a specific timestamp, which we never
    // need in non-test-code as the timestamp comes from the server.
    impl CollectionKeys {
        pub fn to_encrypted_bso_with_timestamp(
            &self,
            root_key: &KeyBundle,
            modified: ServerTimestamp,
        ) -> error::Result<EncryptedBso> {
            let record = CryptoKeysRecord {
                id: "keys".into(),
                collection: "crypto".into(),
                default: self.default.to_b64_array(),
                collections: self
                    .collections
                    .iter()
                    .map(|kv| (kv.0.clone(), kv.1.to_b64_array()))
                    .collect(),
            };
            let mut bso = Payload::from_record(record)?.into_bso("crypto".into());
            bso.modified = modified;
            Ok(bso.encrypt(root_key)?)
        }
    }

    #[test]
    fn test_state_machine_ready_from_empty() {
        let root_key = KeyBundle::new_random().unwrap();
        let keys = CollectionKeys {
            timestamp: 123.4.into(),
            default: KeyBundle::new_random().unwrap(),
            collections: HashMap::new(),
        };
        let client = InMemoryClient {
            info_configuration: mocked_success(InfoConfiguration::default()),
            info_collections: mocked_success(InfoCollections::new(
                vec![("meta", 123.456), ("crypto", 145.0)]
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), value.into()))
                    .collect(),
            )),
            meta_global: mocked_success_ts(
                MetaGlobalRecord {
                    sync_id: "syncIDAAAAAA".to_owned(),
                    storage_version: 5usize,
                    engines: vec![(
                        "bookmarks",
                        MetaGlobalEngine {
                            version: 1usize,
                            sync_id: "syncIDBBBBBB".to_owned(),
                        },
                    )]
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), value))
                    .collect(),
                    declined: vec![],
                },
                999.0,
            ),
            crypto_keys: mocked_success_ts(
                keys.to_encrypted_bso_with_timestamp(&root_key, 888.0.into())
                    .expect("should always work in this test"),
                888.0,
            ),
        };
        let mut pgs = PersistedGlobalState::V2 { declined: None };

        let mut state_machine =
            SetupStateMachine::for_full_sync(&client, &root_key, &mut pgs, &NeverInterrupts);
        assert!(
            state_machine.run_to_ready(None).is_ok(),
            "Should drive state machine to ready"
        );
        assert_eq!(
            state_machine.sequence,
            vec![
                "Initial",
                "InitialWithConfig",
                "InitialWithInfo",
                "InitialWithMetaGlobal",
                "Ready",
            ],
            "Should cycle through all states"
        );
    }
}

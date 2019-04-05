/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;

use crate::client::{SetupStorageClient, Sync15ClientResponse};
use crate::collection_keys::CollectionKeys;
use crate::error::{self, ErrorKind};
use crate::key_bundle::KeyBundle;
use crate::record_types::{MetaGlobalEngine, MetaGlobalRecord};
use crate::request::{InfoCollections, InfoConfiguration};
use crate::util::{random_guid, ServerTimestamp};
use lazy_static::lazy_static;

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

/// XXX - this needs more thought, but this holds stuff the app knows about,
/// or needs to know about, which either comes from meta/global, or is used
/// to create a new meta/global.
/// It can't be treated as "opaque" as apps will be exposing UI to change
/// these items, and will want to know new values after a sync.
/// However, very naive clients who don't expose a way to toggle them
/// could treat them as opaque and could serialize them - but serialization
/// support is not implemented here.

/// One problem here is that the semantics aren't clear - there are 2
/// competing requirements:
/// 1) it is a placeholder for what the "declined engines" were on the last
///    sync, and if meta/global doesn't exist, what we should put in it.
/// 2) it is a way for apps to *change* what the value should be - ie, to
///    decline or undecline and engine.
/// So for now, it's really just a placeholder and a TODO item.
#[derive(Debug, Default)]
pub struct ApplicationState {
    pub declined_engines: Vec<String>,
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
/// and declined engines from the previous record.
// XXX - this is just (a) default engines and (b) declined. Only (b) needs to
// be persisted, so we can move this directly into our `FreshStart`
fn new_global_from_previous(
    previous_global: Option<MetaGlobalRecord>,
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

pub struct SetupStateMachine<'a> {
    client: &'a SetupStorageClient,
    root_key: &'a KeyBundle,
    app_state: &'a ApplicationState,
    allowed_states: Vec<&'static str>,
    sequence: Vec<&'static str>,
}

impl<'a> SetupStateMachine<'a> {
    /// Creates a state machine for a "classic" Sync 1.5 client that supports
    /// all states, including uploading a fresh `meta/global` and `crypto/keys`
    /// after a node reassignment.
    pub fn for_full_sync(
        client: &'a SetupStorageClient,
        root_key: &'a KeyBundle,
        app_state: &'a ApplicationState,
    ) -> SetupStateMachine<'a> {
        SetupStateMachine::with_allowed_states(
            client,
            root_key,
            app_state,
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
        client: &'a SetupStorageClient,
        root_key: &'a KeyBundle,
        app_state: &'a ApplicationState,
    ) -> SetupStateMachine<'a> {
        SetupStateMachine::with_allowed_states(
            client,
            root_key,
            app_state,
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

    /// Creates a state machine for a read-only sync, where the client can't
    /// upload `meta/global` or `crypto/keys`. Useful for clients that only
    /// sync specific collections, like Lockbox.
    pub fn for_readonly_sync(
        client: &'a SetupStorageClient,
        root_key: &'a KeyBundle,
        app_state: &'a ApplicationState,
    ) -> SetupStateMachine<'a> {
        // currently identical to a "fast sync"
        Self::for_fast_sync(client, root_key, app_state)
    }

    fn with_allowed_states(
        client: &'a SetupStorageClient,
        root_key: &'a KeyBundle,
        app_state: &'a ApplicationState,
        allowed_states: Vec<&'static str>,
    ) -> SetupStateMachine<'a> {
        SetupStateMachine {
            client,
            root_key,
            app_state,
            sequence: Vec::new(),
            allowed_states,
        }
    }

    fn advance(&self, from: SetupState) -> error::Result<SetupState> {
        match from {
            // Fetch `info/configuration` with current server limits, and
            // `info/collections` with collection last modified times.
            Initial => {
                let config = match self.client.fetch_info_configuration()? {
                    Sync15ClientResponse::Success { record, .. } => record,
                    Sync15ClientResponse::NotFound { .. } => InfoConfiguration::default(),
                    other => return Err(other.to_storage_error().into()),
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
                    Sync15ClientResponse::NotFound { .. } => Ok(FreshStartRequired {
                        config,
                        old_global: None,
                    }),
                    other => Err(other.to_storage_error().into()),
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
                        Ok(FreshStartRequired {
                            config,
                            old_global: Some(global),
                        })
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
                Sync15ClientResponse::NotFound { .. } => Ok(FreshStartRequired {
                    config,
                    old_global: None,
                }),
                other => Err(other.to_storage_error().into()),
            },

            InitialWithMetaGlobal {
                config,
                collections,
                global,
                global_timestamp,
            } => {
                match self.client.fetch_crypto_keys()? {
                    Sync15ClientResponse::Success { record, .. } => {
                        let new_keys = CollectionKeys::from_encrypted_bso(record, self.root_key)?;
                        // Note that collection/keys is itself a bso, so the
                        // json body also carries the timestamp. If they aren't
                        // identical something has screwed up. Sadly though
                        // our tests get upset if we assert this.
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
                    Sync15ClientResponse::NotFound { .. } => Ok(FreshStartRequired {
                        config,
                        old_global: Some(global),
                    }),
                    other => Err(other.to_storage_error().into()),
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

            FreshStartRequired { config, old_global } => {
                // Wipe the server.
                self.client.wipe_all_remote()?;

                // Upload a fresh `meta/global`...
                let new_global = new_global_from_previous(old_global)?;
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
        old_global: Option<MetaGlobalRecord>,
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

    use crate::bso_record::{BsoRecord, EncryptedBso, EncryptedPayload};

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
                keys.to_encrypted_bso(&root_key)
                    .expect("should always work in this test"),
                888.0,
            ),
        };
        let app_state = ApplicationState::default();

        let mut state_machine = SetupStateMachine::for_full_sync(&client, &root_key, &app_state);
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

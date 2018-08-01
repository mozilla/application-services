/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::{HashMap, HashSet};

use bso_record::BsoRecord;
use client::SetupStorageClient;
use collection_keys::CollectionKeys;
use error::{self, ErrorKind};
use key_bundle::KeyBundle;
use record_types::{MetaGlobalEngine, MetaGlobalRecord};
use request::{InfoCollections, InfoConfiguration};
use util::{random_guid, ServerTimestamp, SERVER_EPOCH};

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

/// Holds global Sync state, including server upload limits, and the
/// last-fetched collection modified times, `meta/global` record, and
/// collection encryption keys.
#[derive(Debug, Default)]
pub struct GlobalState {
    pub config: InfoConfiguration,
    pub collections: InfoCollections,
    pub global: Option<BsoRecord<MetaGlobalRecord>>,
    pub keys: Option<CollectionKeys>,
    pub engine_state_changes: Vec<EngineStateChange>,
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

    /// Returns a set of all engine names that should be reset locally.
    pub fn engines_that_need_local_reset(&self) -> HashSet<String> {
        let all_engines = self.global
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
                EngineStateChange::ResetAll => {
                    engines_to_reset.reserve(all_engines.len());
                    for name in all_engines.iter() {
                        engines_to_reset.insert(name.to_string());
                    }
                }
                EngineStateChange::ResetAllExcept(except) => {
                    for name in all_engines.difference(except) {
                        engines_to_reset.insert(name.to_string());
                    }
                }
                _ => {}
            }
        }
        engines_to_reset
    }
}

fn resolve_global(
    previous_state: GlobalState,
    new_global: BsoRecord<MetaGlobalRecord>,
) -> GlobalState {
    let mut changes = previous_state.engine_state_changes;
    let (new_global, previous_keys) = match &previous_state.global {
        Some(previous_global) => {
            if previous_global.sync_id != new_global.sync_id {
                changes.push(EngineStateChange::ResetAll);
                (new_global, None)
            } else {
                let mut new_changes =
                    engine_state_changes_from_new_global(previous_global, &new_global);
                changes.append(&mut new_changes);
                (new_global, previous_state.keys)
            }
        }
        None => {
            changes.push(EngineStateChange::ResetAll);
            (new_global, None)
        }
    };
    GlobalState {
        config: previous_state.config,
        collections: previous_state.collections,
        global: Some(new_global),
        keys: previous_keys,
        engine_state_changes: changes,
    }
}

fn engine_state_changes_from_new_global(
    previous_global: &MetaGlobalRecord,
    new_global: &MetaGlobalRecord,
) -> Vec<EngineStateChange> {
    let mut changes = Vec::new();

    let previous_engine_names = previous_global.engines.keys().collect::<HashSet<&String>>();
    let new_engine_names = new_global.engines.keys().collect::<HashSet<&String>>();

    // Disable any local engines that aren't mentioned
    // in the new `meta/global`.
    for name in previous_engine_names.difference(&new_engine_names) {
        changes.push(EngineStateChange::Disable(name.to_string()));
    }

    // Enable any new engines that aren't mentioned in
    // the locally cached `meta/global`.
    for name in new_engine_names.difference(&previous_engine_names) {
        changes.push(EngineStateChange::Enable(name.to_string()));
    }

    // Reset engines with sync ID changes.
    for name in new_engine_names.intersection(&previous_engine_names) {
        let previous_engine = previous_global.engines.get(*name).unwrap();
        let new_engine = new_global.engines.get(*name).unwrap();
        if previous_engine.sync_id != new_engine.sync_id {
            changes.push(EngineStateChange::Reset(name.to_string()));
        }
    }

    changes
}

fn resolve_keys(previous_state: GlobalState, new_keys: CollectionKeys) -> GlobalState {
    let mut changes = previous_state.engine_state_changes;
    match &previous_state.keys {
        Some(previous_global) => {
            if new_keys.default == previous_global.default {
                // The default bundle is the same, so only reset
                // engines with different collection-specific keys.
                for (collection, key_bundle) in &previous_global.collections {
                    if key_bundle != new_keys.key_for_collection(collection) {
                        changes.push(EngineStateChange::Reset(collection.to_string()));
                    }
                }
                for (collection, key_bundle) in &new_keys.collections {
                    if key_bundle != previous_global.key_for_collection(collection) {
                        changes.push(EngineStateChange::Reset(collection.to_string()));
                    }
                }
            } else {
                // The default bundle changed, so reset all engines
                // except those with the same collection-specific
                // keys.
                let mut except = HashSet::new();
                for (collection, key_bundle) in &previous_global.collections {
                    if key_bundle == new_keys.key_for_collection(collection) {
                        except.insert(collection.to_string());
                    }
                }
                for (collection, key_bundle) in &new_keys.collections {
                    if key_bundle != previous_global.key_for_collection(collection) {
                        except.insert(collection.to_string());
                    }
                }
                changes.push(EngineStateChange::ResetAllExcept(except));
            }
        }
        None => changes.push(EngineStateChange::ResetAll),
    }
    GlobalState {
        config: previous_state.config,
        collections: previous_state.collections,
        global: previous_state.global,
        keys: Some(new_keys),
        engine_state_changes: changes,
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

pub struct SetupStateMachine<'client, 'keys> {
    client: &'client SetupStorageClient,
    root_key: &'keys KeyBundle,
    allowed_states: Vec<&'static str>,
    sequence: Vec<&'static str>,
}

impl<'client, 'keys> SetupStateMachine<'client, 'keys> {
    /// Creates a state machine for a "classic" Sync 1.5 client that supports
    /// all states, including uploading a fresh `meta/global` and `crypto/keys`
    /// after a node reassignment.
    pub fn for_full_sync(
        client: &'client SetupStorageClient,
        root_key: &'keys KeyBundle,
    ) -> SetupStateMachine<'client, 'keys> {
        SetupStateMachine::with_allowed_states(
            client,
            root_key,
            vec![
                "InitialWithLiveToken",
                "InitialWithLiveTokenAndConfig",
                "InitialWithLiveTokenAndInfo",
                "NeedsFreshMetaGlobal",
                "HasMetaGlobal",
                "ResolveMetaGlobal",
                "NeedsFreshCryptoKeys",
                "Ready",
                "FreshStartRequired",
            ],
        )
    }

    /// Creates a state machine for a fast sync, which only uses locally
    /// cached global state, and bails if `meta/global` or `crypto/keys`
    /// are missing or out-of-date. This is useful in cases where it's
    /// important to get to ready as quickly as possible, like syncing before
    /// sleep, or when conserving time or battery life.
    pub fn for_fast_sync(
        client: &'client SetupStorageClient,
        root_key: &'keys KeyBundle,
    ) -> SetupStateMachine<'client, 'keys> {
        SetupStateMachine::with_allowed_states(
            client,
            root_key,
            vec![
                "InitialWithLiveToken",
                "InitialWithLiveTokenAndConfig",
                "InitialWithLiveTokenAndInfo",
                "HasMetaGlobal",
                "Ready",
            ],
        )
    }

    /// Creates a state machine for a read-only sync, where the client can't
    /// upload `meta/global` or `crypto/keys`. Useful for clients that only
    /// sync specific collections, like Lockbox.
    pub fn for_readonly_sync(
        client: &'client SetupStorageClient,
        root_key: &'keys KeyBundle,
    ) -> SetupStateMachine<'client, 'keys> {
        SetupStateMachine::with_allowed_states(
            client,
            root_key,
            vec![
                "InitialWithLiveToken",
                "InitialWithLiveTokenAndConfig",
                "InitialWithLiveTokenAndInfo",
                "NeedsFreshMetaGlobal",
                "HasMetaGlobal",
                "ResolveMetaGlobal",
                "NeedsFreshCryptoKeys",
                "Ready",
            ],
        )
    }

    fn with_allowed_states(
        client: &'client SetupStorageClient,
        root_key: &'keys KeyBundle,
        allowed_states: Vec<&'static str>,
    ) -> SetupStateMachine<'client, 'keys> {
        SetupStateMachine {
            client,
            root_key,
            sequence: Vec::new(),
            allowed_states,
        }
    }

    fn advance(&self, from: SetupState) -> error::Result<SetupState> {
        match from {
            // Fetch `info/configuration` with current server limits, and
            // `info/collections` with collection last modified times.
            InitialWithLiveToken(state) => {
                let config = self.client
                    .fetch_info_configuration()
                    .unwrap_or(state.config);
                Ok(InitialWithLiveTokenAndConfig(GlobalState {
                    config,
                    collections: state.collections,
                    global: state.global,
                    keys: state.keys,
                    engine_state_changes: Vec::new(),
                }))
            }

            InitialWithLiveTokenAndConfig(state) => {
                let collections = self.client.fetch_info_collections()?;
                Ok(InitialWithLiveTokenAndInfo(GlobalState {
                    config: state.config,
                    collections,
                    global: state.global,
                    keys: state.keys,
                    engine_state_changes: state.engine_state_changes,
                }))
            }

            // Compare local and remote `meta/global` timestamps to determine
            // if our locally cached `meta/global` is up-to-date.
            InitialWithLiveTokenAndInfo(state) => {
                let action = {
                    let local = state.global.as_ref().map(|global| &global.modified);
                    let remote = state.collections.get("meta");
                    FetchAction::from_modified(local, remote)
                };
                Ok(match action {
                    // Hooray, we don't need to fetch `meta/global`. Skip to
                    // the next state.
                    FetchAction::Skip => HasMetaGlobal(state),
                    // Our `meta/global` is out of date, or isn't cached
                    // locally, so we need to fetch it from the server.
                    FetchAction::Fetch => NeedsFreshMetaGlobal(state),
                    // We have a `meta/global` record in our cache, but not on
                    // the server. This likely means we're the first client to
                    // sync after a node reassignment. Invalidate our cached
                    // `meta/global` and `crypto/keys`, and try to fetch
                    // `meta/global` from the server anyway. If another client
                    // wins the race, we'll fetch its `meta/global`; if not,
                    // we'll fail and upload our own.
                    FetchAction::InvalidateThenUpload => NeedsFreshMetaGlobal(GlobalState {
                        config: state.config,
                        collections: state.collections,
                        global: None,
                        keys: None,
                        engine_state_changes: state.engine_state_changes,
                    }),
                })
            }

            // Fetch `meta/global` from the server.
            NeedsFreshMetaGlobal(state) => match self.client.fetch_meta_global() {
                Ok(new_global) => Ok(ResolveMetaGlobal(state, new_global)),
                Err(err) => match err.kind() {
                    ErrorKind::NoMetaGlobal { .. } => Ok(FreshStartRequired(state)),
                    _ => Err(err),
                },
            },

            // Reconcile the server's `meta/global` with our locally cached
            // `meta/global`, if any.
            ResolveMetaGlobal(state, new_global) => {
                // If the server has a newer storage version, we can't
                // sync until our client is updated.
                if new_global.payload.storage_version > STORAGE_VERSION {
                    return Err(ErrorKind::ClientUpgradeRequired.into());
                }

                // If the server has an older storage version, wipe and
                // reupload.
                if new_global.payload.storage_version < STORAGE_VERSION {
                    return Ok(FreshStartRequired(state));
                }

                let new_state = resolve_global(state, new_global);
                Ok(HasMetaGlobal(new_state))
            }

            // Check if our locally cached `crypto/keys` collection is
            // up-to-date.
            HasMetaGlobal(state) => {
                // TODO(lina): Check if we've enabled or disabled any engines
                // locally, and update `m/g` to reflect that.
                let action = {
                    let local = state.keys.as_ref().map(|keys| &keys.timestamp);
                    let remote = state.collections.get("crypto");
                    FetchAction::from_modified(local, remote)
                };
                Ok(match action {
                    // If `crypto/keys` is up-to-date, we're ready to go!
                    FetchAction::Skip => Ready(state),
                    // We need to fetch and cache new keys.
                    FetchAction::Fetch => NeedsFreshCryptoKeys(state),
                    // We need to invalidate our locally cached `crypto/keys`,
                    // then try to fetch new keys, and reupload if fetching
                    // fails.
                    FetchAction::InvalidateThenUpload => NeedsFreshCryptoKeys(GlobalState {
                        config: state.config,
                        collections: state.collections,
                        global: state.global,
                        keys: None,
                        engine_state_changes: state.engine_state_changes,
                    }),
                })
            }

            NeedsFreshCryptoKeys(state) => {
                match self.client.fetch_crypto_keys() {
                    Ok(encrypted_bso) => {
                        let new_keys =
                            CollectionKeys::from_encrypted_bso(encrypted_bso, self.root_key)?;
                        let new_state = resolve_keys(state, new_keys);
                        Ok(Ready(new_state))
                    }
                    Err(err) => match err.kind() {
                        // If the server doesn't have a `crypto/keys`, start over
                        // and reupload our `meta/global` and `crypto/keys`.
                        ErrorKind::NoCryptoKeys { .. } => Ok(FreshStartRequired(state)),
                        _ => Err(err),
                    },
                }
            }

            Ready(state) => Ok(Ready(state)),

            FreshStartRequired(state) => {
                // Wipe the server.
                self.client.wipe_all_remote()?;

                // Upload a fresh `meta/global`...
                let new_global = BsoRecord::new_record(
                    "global".into(),
                    "meta".into(),
                    new_global_from_previous(state.global)?,
                );
                self.client.put_meta_global(&new_global)?;

                // ...And a fresh `crypto/keys`. Note that we'll update the
                // global state when we go around the state machine again,
                // not here.
                let new_keys = CollectionKeys::new_random()?.to_encrypted_bso(&self.root_key)?;
                self.client.put_crypto_keys(&new_keys)?;

                // TODO(lina): Can we pass along server timestamps from the PUTs
                // above, and avoid re-fetching the `m/g` and `c/k` we just
                // uploaded?
                Ok(InitialWithLiveTokenAndConfig(GlobalState {
                    config: state.config,
                    collections: InfoCollections::default(),
                    global: None,
                    keys: None,
                    engine_state_changes: vec![EngineStateChange::ResetAll],
                }))
            }
        }
    }

    /// Runs through the state machine to the ready state.
    pub fn to_ready(&mut self, state: GlobalState) -> error::Result<GlobalState> {
        let mut s = InitialWithLiveToken(state);
        loop {
            let label = &s.label();
            match s {
                Ready(state) => {
                    self.sequence.push(label);
                    return Ok(state);
                }
                // If we already started over once before, we're likely in a
                // cycle, and should try again later. Like the iOS state
                // machine, other cycles aren't a problem; we'll cycle through
                // earlier states if we need to reupload `meta/global` or
                // `crypto/keys`.
                FreshStartRequired(_) if self.sequence.contains(&label) => {
                    return Err(ErrorKind::SetupStateCycleError.into());
                }
                previous_s => {
                    if !self.allowed_states.contains(&label) {
                        return Err(ErrorKind::DisallowedStateError(&label).into());
                    }
                    self.sequence.push(label);
                    s = self.advance(previous_s)?;
                }
            }
        }
    }
}

/// States in the remote setup process.
/// TODO(lina): Add link once #56 is merged.
#[derive(Debug)]
enum SetupState {
    InitialWithLiveToken(GlobalState),
    InitialWithLiveTokenAndConfig(GlobalState),
    InitialWithLiveTokenAndInfo(GlobalState),
    NeedsFreshMetaGlobal(GlobalState),
    HasMetaGlobal(GlobalState),
    ResolveMetaGlobal(GlobalState, BsoRecord<MetaGlobalRecord>),
    NeedsFreshCryptoKeys(GlobalState),
    Ready(GlobalState),
    FreshStartRequired(GlobalState),
}

impl SetupState {
    fn label(&self) -> &'static str {
        match self {
            InitialWithLiveToken(_) => "InitialWithLiveToken",
            InitialWithLiveTokenAndConfig(_) => "InitialWithLiveTokenAndConfig",
            InitialWithLiveTokenAndInfo(_) => "InitialWithLiveTokenAndInfo",
            NeedsFreshMetaGlobal(_) => "NeedsFreshMetaGlobal",
            HasMetaGlobal(_) => "HasMetaGlobal",
            ResolveMetaGlobal(_, _) => "ResolveMetaGlobal",
            NeedsFreshCryptoKeys(_) => "NeedsFreshCryptoKeys",
            Ready(_) => "Ready",
            FreshStartRequired(_) => "FreshStartRequired",
        }
    }
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
#[derive(Debug)]
pub enum EngineStateChange {
    ResetAll,
    ResetAllExcept(HashSet<String>),
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
        info_collections: error::Result<InfoCollections>,
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

        fn fetch_info_collections(&self) -> error::Result<InfoCollections> {
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

        fn wipe_all_remote(&self) -> error::Result<()> {
            Ok(())
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
            info_configuration: Ok(InfoConfiguration::default()),
            info_collections: Ok(InfoCollections::new(
                vec![("meta", 123.456), ("crypto", 145.0)]
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), value.into()))
                    .collect(),
            )),
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
                    ].into_iter()
                        .map(|(key, value)| (key.to_owned(), value.into()))
                        .collect(),
                    declined: vec![],
                },
            }),
            crypto_keys: keys.to_encrypted_bso(&root_key),
        };

        let state = GlobalState::default();
        let mut state_machine = SetupStateMachine::for_full_sync(&client, &root_key);
        assert!(
            state_machine.to_ready(state).is_ok(),
            "Should drive state machine to ready"
        );
        assert_eq!(
            state_machine.sequence,
            vec![
                "InitialWithLiveToken",
                "InitialWithLiveTokenAndConfig",
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

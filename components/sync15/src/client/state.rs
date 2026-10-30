/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::{HashMap, HashSet};

use super::CollectionKeys;
use super::request::{InfoCollections, InfoConfiguration};
use super::storage_client::{SetupStorageClient, Sync15ClientResponse};
use crate::EncryptedPayload;
use crate::bso::OutgoingEncryptedBso;
use crate::error::{self, Error as ErrorKind, ErrorResponse, debug, info, trace, warn};
use crate::record_types::{MetaGlobalEngine, MetaGlobalRecord, SyncIdsRecord};
use crate::{Guid, KeyBundle, ServerTimestamp};
use interrupt_support::Interruptee;
use serde_derive::*;

use self::SetupState::*;

const STORAGE_VERSION: usize = 5;

/// Maps names to storage versions for engines to include in a fresh
/// `meta/global` record. We include engines that we don't implement
/// because they'll be disabled on other clients if we omit them
/// (bug 1479929).
/// Versions are nebulous, mismatches are handled poorly, if at all.
const DEFAULT_ENGINES: &[(&str, usize)] = &[
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
const DEFAULT_DECLINED: &[&str] = &[];

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
    // V1 was when we persisted the entire GlobalState, keys and all!
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

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct EngineChangesNeeded {
    pub local_resets: HashSet<String>,
    pub remote_wipes: HashSet<String>,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct RemoteEngineState {
    info_collections: HashSet<String>,
    declined: HashSet<String>,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct EngineStateInput {
    local_declined: HashSet<String>,
    remote: Option<RemoteEngineState>,
    user_changes: HashMap<String, bool>,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct EngineStateOutput {
    // The new declined.
    declined: HashSet<String>,
    // Which engines need resets or wipes.
    changes_needed: EngineChangesNeeded,
}

fn compute_engine_states(input: EngineStateInput) -> EngineStateOutput {
    use super::util::*;
    debug!("compute_engine_states: input {:?}", input);
    let (must_enable, must_disable) = partition_by_value(&input.user_changes);
    let have_remote = input.remote.is_some();
    let RemoteEngineState {
        info_collections,
        declined: remote_declined,
    } = input.remote.clone().unwrap_or_default();

    let both_declined_and_remote = set_intersection(&info_collections, &remote_declined);
    if !both_declined_and_remote.is_empty() {
        // Should we wipe these too?
        warn!(
            "Remote state contains engines which are in both info/collections and meta/global's declined: {:?}",
            both_declined_and_remote,
        );
    }

    let most_recent_declined_list = if have_remote {
        &remote_declined
    } else {
        &input.local_declined
    };

    let result_declined = set_difference(
        &set_union(most_recent_declined_list, &must_disable),
        &must_enable,
    );

    let output = EngineStateOutput {
        changes_needed: EngineChangesNeeded {
            // Anything now declined which wasn't in our declined list before gets a reset.
            local_resets: set_difference(&result_declined, &input.local_declined),
            // Anything remote that we just declined gets a wipe. In the future
            // we might want to consider wiping things in both remote declined
            // and info/collections, but we'll let other clients pick up their
            // own mess for now.
            remote_wipes: set_intersection(&info_collections, &must_disable),
        },
        declined: result_declined,
    };
    // No PII here and this helps debug problems.
    debug!("compute_engine_states: output {:?}", output);
    output
}

impl PersistedGlobalState {
    fn set_declined(&mut self, new_declined: Vec<String>) {
        match self {
            Self::V2 { declined } => *declined = Some(new_declined),
        }
    }
    pub(crate) fn get_declined(&self) -> &[String] {
        match self {
            Self::V2 { declined: Some(d) } => d,
            Self::V2 { declined: None } => &[],
        }
    }
}

/// Holds global Sync state, including server upload limits, the
/// last-fetched collection modified times, `meta/global` record, and
/// an encrypted copy of the crypto/keys resource (avoids keeping them
/// in memory longer than necessary; avoids key mismatches by ensuring the same KeyBundle
/// is used for both the keys and encrypted payloads.)
#[derive(Debug, Clone)]
pub struct GlobalState {
    pub config: InfoConfiguration,
    pub collections: InfoCollections,
    pub global: MetaGlobalRecord,
    pub global_timestamp: ServerTimestamp,
    /// The timestamp of the `meta` collection when we last saw it. This is
    /// not always `global_timestamp`, because meta/global isn't the only record
    /// in that collection.
    pub meta_timestamp: ServerTimestamp,
    pub keys: EncryptedPayload,
    pub keys_timestamp: ServerTimestamp,
}

/// Creates a fresh `meta/global` record, using the default engine selections,
/// and declined engines from our PersistedGlobalState.
fn new_global(pgs: &PersistedGlobalState) -> MetaGlobalRecord {
    let sync_id = Guid::random();
    // We only need our PersistedGlobalState to fill out a new meta/global - if
    // we previously saw a meta/global then we would have updated it with what
    // it was at the time.
    let declined = match pgs {
        PersistedGlobalState::V2 { declined: Some(d) } => d.clone(),
        _ => DEFAULT_DECLINED.iter().map(ToString::to_string).collect(),
    };
    // Declined engines must not appear in `engines`, otherwise other clients
    // will treat them as enabled.
    let engines = DEFAULT_ENGINES
        .iter()
        .filter(|(name, _)| !declined.iter().any(|d| d == name))
        .map(|(name, version)| {
            (
                (*name).to_string(),
                MetaGlobalEngine {
                    version: *version,
                    sync_id: Guid::random(),
                },
            )
        })
        .collect();

    MetaGlobalRecord {
        sync_id,
        storage_version: STORAGE_VERSION,
        engines,
        declined,
    }
}

/// Whether `fixup_meta_global` will need to add any engines to `meta/global`.
fn has_missing_engines(global: &MetaGlobalRecord) -> bool {
    DEFAULT_ENGINES.iter().any(|(name, _)| {
        !global.engines.contains_key(*name) && !global.declined.iter().any(|c| c == name)
    })
}

/// Fetches `meta/syncids`. A missing or unparseable record just means there's
/// nothing to adopt, but other errors are fatal - minting new sync IDs because
/// of a transient error would force clients using the record to reset.
fn fetch_sync_ids(client: &dyn SetupStorageClient) -> error::Result<Option<SyncIdsRecord>> {
    match client.fetch_sync_ids()? {
        Sync15ClientResponse::Success { record, .. } => Ok(record),
        Sync15ClientResponse::Error(ErrorResponse::NotFound { .. }) => Ok(None),
        other => Err(other.create_storage_error()),
    }
}

fn fixup_meta_global(global: &mut MetaGlobalRecord, sync_ids: Option<&SyncIdsRecord>) -> bool {
    let mut changed_any = false;
    for &(name, version) in DEFAULT_ENGINES.iter() {
        let had_engine = global.engines.contains_key(name);
        let should_have_engine = !global.declined.iter().any(|c| c == name);
        if had_engine != should_have_engine {
            if should_have_engine {
                // Adopt the sync ID other clients are already using, if there is one.
                let engine = match sync_ids.and_then(|r| r.usable_entry(global, name, version)) {
                    Some(engine) => {
                        info!(
                            "Adopting the sync ID for engine {:?} from meta/syncids",
                            name
                        );
                        engine.clone()
                    }
                    None => {
                        debug!("SyncID for engine {:?} was missing", name);
                        MetaGlobalEngine {
                            version,
                            sync_id: Guid::random(),
                        }
                    }
                };
                global.engines.insert(name.to_string(), engine);
            } else {
                debug!("SyncID for engine {:?} was present, but shouldn't be", name);
                global.engines.remove(name);
            }
            changed_any = true;
        }
    }
    changed_any
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
    engine_updates: Option<&'a HashMap<String, bool>>,
    interruptee: &'a dyn Interruptee,
    pub(crate) changes_needed: Option<EngineChangesNeeded>,
}

impl<'a> SetupStateMachine<'a> {
    /// Creates a state machine for a "classic" Sync 1.5 client that supports
    /// all states, including uploading a fresh `meta/global` and `crypto/keys`
    /// after a node reassignment.
    pub fn for_full_sync(
        client: &'a dyn SetupStorageClient,
        root_key: &'a KeyBundle,
        pgs: &'a mut PersistedGlobalState,
        engine_updates: Option<&'a HashMap<String, bool>>,
        interruptee: &'a dyn Interruptee,
    ) -> SetupStateMachine<'a> {
        SetupStateMachine::with_allowed_states(
            client,
            root_key,
            pgs,
            interruptee,
            engine_updates,
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

    fn with_allowed_states(
        client: &'a dyn SetupStorageClient,
        root_key: &'a KeyBundle,
        pgs: &'a mut PersistedGlobalState,
        interruptee: &'a dyn Interruptee,
        engine_updates: Option<&'a HashMap<String, bool>>,
        allowed_states: Vec<&'static str>,
    ) -> SetupStateMachine<'a> {
        SetupStateMachine {
            client,
            root_key,
            pgs,
            sequence: Vec::new(),
            allowed_states,
            engine_updates,
            interruptee,
            changes_needed: None,
        }
    }

    fn advance(&mut self, from: SetupState) -> error::Result<SetupState> {
        match from {
            // Fetch `info/configuration` with current server limits, and
            // `info/collections` with collection last modified times.
            Initial => {
                let config = match self.client.fetch_info_configuration()? {
                    Sync15ClientResponse::Success { record, .. } => record,
                    Sync15ClientResponse::Error(ErrorResponse::NotFound { .. }) => {
                        InfoConfiguration::default()
                    }
                    other => return Err(other.create_storage_error()),
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
                    Sync15ClientResponse::Error(ErrorResponse::NotFound { .. }) => {
                        Ok(FreshStartRequired { config })
                    }
                    other => Err(other.create_storage_error()),
                }
            }

            InitialWithInfo {
                config,
                collections,
            } => {
                match self.client.fetch_meta_global()? {
                    Sync15ClientResponse::Success {
                        record: mut global,
                        last_modified: mut global_timestamp,
                        ..
                    } => {
                        // If the server has a newer storage version, we can't
                        // sync until our client is updated.
                        if global.storage_version > STORAGE_VERSION {
                            return Err(ErrorKind::ClientUpgradeRequired);
                        }

                        // If the server has an older storage version, wipe and
                        // reupload.
                        if global.storage_version < STORAGE_VERSION {
                            Ok(FreshStartRequired { config })
                        } else {
                            info!(
                                "Have info/collections and meta/global. Computing new engine states"
                            );
                            let initial_global_declined: HashSet<String> =
                                global.declined.iter().cloned().collect();
                            let local_declined: HashSet<String> =
                                self.pgs.get_declined().iter().cloned().collect();
                            // Clients which allow engines to be enabled or disabled
                            // per device write a meta/global with no engines and no
                            // declined engines. That empty list doesn't reflect
                            // anyone's choices, so rather than adopting it (which
                            // would enable everything), we keep our own and publish
                            // them, just as other clients do.
                            let remote_declined = if global.engines.is_empty()
                                && initial_global_declined.is_empty()
                            {
                                info!(
                                    "meta/global has no engines or declined engines - keeping our declined engines"
                                );
                                local_declined.clone()
                            } else {
                                initial_global_declined.clone()
                            };
                            let result = compute_engine_states(EngineStateInput {
                                local_declined,
                                user_changes: self.engine_updates.cloned().unwrap_or_default(),
                                remote: Some(RemoteEngineState {
                                    declined: remote_declined,
                                    info_collections: collections.keys().cloned().collect(),
                                }),
                            });
                            // If the declined engines differ from remote, fix that.
                            let fixed_declined = if result.declined != initial_global_declined {
                                global.declined = result.declined.iter().cloned().collect();
                                info!(
                                    "Uploading new declined {:?} to meta/global with timestamp {:?}",
                                    global.declined, global_timestamp,
                                );
                                true
                            } else {
                                false
                            };
                            // Fetch any sync IDs we can adopt before we change
                            // any of our own state, so a failure leaves no trace.
                            let sync_ids = if has_missing_engines(&global) {
                                fetch_sync_ids(self.client)?
                            } else {
                                None
                            };
                            // Persist the new declined.
                            self.pgs
                                .set_declined(result.declined.iter().cloned().collect());
                            // If there are missing syncIds, we need to fix those as well
                            let fixed_ids = if fixup_meta_global(&mut global, sync_ids.as_ref()) {
                                info!(
                                    "Uploading corrected meta/global with timestamp {:?}",
                                    global_timestamp,
                                );
                                true
                            } else {
                                false
                            };

                            let mut meta_timestamp =
                                collections.get("meta").copied().unwrap_or_default();
                            if fixed_declined || fixed_ids {
                                global_timestamp =
                                    self.client.put_meta_global(global_timestamp, &global)?;
                                meta_timestamp = global_timestamp;
                                debug!("new global_timestamp: {:?}", global_timestamp);
                            }
                            // Update the set of changes needed.
                            if self.changes_needed.is_some() {
                                // Should never happen (we prevent state machine
                                // loops elsewhere) but if it did, the info is stale
                                // anyway.
                                warn!("Already have a set of changes needed, Overwriting...");
                            }
                            self.changes_needed = Some(result.changes_needed);
                            Ok(InitialWithMetaGlobal {
                                config,
                                collections,
                                global,
                                global_timestamp,
                                meta_timestamp,
                            })
                        }
                    }
                    Sync15ClientResponse::Error(ErrorResponse::NotFound { .. }) => {
                        Ok(FreshStartRequired { config })
                    }
                    other => Err(other.create_storage_error()),
                }
            }

            InitialWithMetaGlobal {
                config,
                collections,
                global,
                global_timestamp,
                meta_timestamp,
            } => {
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
                        assert_eq!(last_modified, record.envelope.modified);
                        let state = GlobalState {
                            config,
                            collections,
                            global,
                            global_timestamp,
                            meta_timestamp,
                            keys: record.payload,
                            keys_timestamp: last_modified,
                        };
                        Ok(Ready { state })
                    }
                    // If the server doesn't have a `crypto/keys`, start over
                    // and reupload our `meta/global` and `crypto/keys`.
                    Sync15ClientResponse::Error(ErrorResponse::NotFound { .. }) => {
                        Ok(FreshStartRequired { config })
                    }
                    other => Err(other.create_storage_error()),
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
                    if self.engine_updates.is_none()
                        && is_same_timestamp(old_state.meta_timestamp, &collections, "meta")
                        && is_same_timestamp(old_state.keys_timestamp, &collections, "crypto")
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
                info!("Fresh start: wiping remote");
                self.client.wipe_all_remote()?;

                // Upload a fresh `meta/global`...
                info!("Uploading meta/global");
                let computed = compute_engine_states(EngineStateInput {
                    local_declined: self.pgs.get_declined().iter().cloned().collect(),
                    user_changes: self.engine_updates.cloned().unwrap_or_default(),
                    remote: None,
                });
                self.pgs
                    .set_declined(computed.declined.iter().cloned().collect());

                self.changes_needed = Some(computed.changes_needed);

                let new_global = new_global(self.pgs);

                self.client
                    .put_meta_global(ServerTimestamp::default(), &new_global)?;

                // ...And a fresh `crypto/keys`.
                let new_keys = CollectionKeys::new_random()?.to_encrypted_payload(self.root_key)?;
                let bso = OutgoingEncryptedBso::new(Guid::new("keys").into(), new_keys);
                self.client
                    .put_crypto_keys(ServerTimestamp::default(), &bso)?;

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
            trace!("global state: {:?}", label);
            match s {
                Ready { state } => {
                    self.sequence.push(label);
                    return Ok(state);
                }
                // If we already started over once before, we're likely in a
                // cycle, and should try again later. Intermediate states
                // aren't a problem, just the initial ones.
                FreshStartRequired { .. } | WithPreviousState { .. } | Initial => {
                    if self.sequence.contains(label) {
                        // Is this really the correct error?
                        return Err(ErrorKind::SetupRace);
                    }
                }
                _ => {
                    if !self.allowed_states.contains(label) {
                        return Err(ErrorKind::SetupRequired);
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
        meta_timestamp: ServerTimestamp,
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
            Initial => "Initial",
            InitialWithConfig { .. } => "InitialWithConfig",
            InitialWithInfo { .. } => "InitialWithInfo",
            InitialWithMetaGlobal { .. } => "InitialWithMetaGlobal",
            Ready { .. } => "Ready",
            WithPreviousState { .. } => "WithPreviousState",
            FreshStartRequired { .. } => "FreshStartRequired",
        }
    }
}

/// Whether we should skip fetching an item. Used when we already have timestamps
/// and want to check if we should reuse our existing state. The state's fairly
/// cheap to recreate and very bad to use if it is wrong, so we insist on the
/// *exact* timestamp matching and not a simple "later than" check.
fn is_same_timestamp(local: ServerTimestamp, collections: &InfoCollections, key: &str) -> bool {
    collections.get(key).is_some_and(|ts| local == *ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::bso::{IncomingEncryptedBso, IncomingEnvelope};
    use interrupt_support::NeverInterrupts;
    use std::cell::{Cell, RefCell};

    struct InMemoryClient {
        info_configuration: error::Result<Sync15ClientResponse<InfoConfiguration>>,
        info_collections: error::Result<Sync15ClientResponse<InfoCollections>>,
        meta_global: error::Result<Sync15ClientResponse<MetaGlobalRecord>>,
        crypto_keys: error::Result<Sync15ClientResponse<IncomingEncryptedBso>>,
        sync_ids: Sync15ClientResponse<Option<SyncIdsRecord>>,
        sync_ids_fetches: Cell<usize>,
        // If set, uploaded meta/global records are recorded here rather than
        // checked against what the original tests expect.
        uploaded_globals: Option<RefCell<Vec<MetaGlobalRecord>>>,
    }

    impl SetupStorageClient for InMemoryClient {
        fn fetch_info_configuration(
            &self,
        ) -> error::Result<Sync15ClientResponse<InfoConfiguration>> {
            match &self.info_configuration {
                Ok(client_response) => Ok(client_response.clone()),
                Err(_) => Ok(Sync15ClientResponse::Error(ErrorResponse::ServerError {
                    status: 500,
                    route: "test/path".into(),
                })),
            }
        }

        fn fetch_info_collections(&self) -> error::Result<Sync15ClientResponse<InfoCollections>> {
            match &self.info_collections {
                Ok(collections) => Ok(collections.clone()),
                Err(_) => Ok(Sync15ClientResponse::Error(ErrorResponse::ServerError {
                    status: 500,
                    route: "test/path".into(),
                })),
            }
        }

        fn fetch_meta_global(&self) -> error::Result<Sync15ClientResponse<MetaGlobalRecord>> {
            match &self.meta_global {
                Ok(global) => Ok(global.clone()),
                // TODO(lina): Special handling for 404s, we want to ensure we
                // handle missing keys and other server errors correctly.
                Err(_) => Ok(Sync15ClientResponse::Error(ErrorResponse::ServerError {
                    status: 500,
                    route: "test/path".into(),
                })),
            }
        }

        fn fetch_sync_ids(&self) -> error::Result<Sync15ClientResponse<Option<SyncIdsRecord>>> {
            self.sync_ids_fetches.set(self.sync_ids_fetches.get() + 1);
            Ok(self.sync_ids.clone())
        }

        fn put_meta_global(
            &self,
            xius: ServerTimestamp,
            global: &MetaGlobalRecord,
        ) -> error::Result<ServerTimestamp> {
            if let Some(uploaded) = &self.uploaded_globals {
                uploaded.borrow_mut().push(global.clone());
                return Ok(ServerTimestamp(xius.0 + 1));
            }
            // Ensure that the meta/global record we uploaded is "fixed up"
            assert!(
                DEFAULT_ENGINES
                    .iter()
                    .filter(|e| e.0 != "logins")
                    .all(|&(k, _v)| global.engines.contains_key(k))
            );
            assert!(!global.engines.contains_key("logins"));
            assert_eq!(global.declined, vec!["logins".to_string()]);
            // return a different timestamp.
            Ok(ServerTimestamp(xius.0 + 1))
        }

        fn fetch_crypto_keys(&self) -> error::Result<Sync15ClientResponse<IncomingEncryptedBso>> {
            match &self.crypto_keys {
                Ok(Sync15ClientResponse::Success {
                    status,
                    record,
                    last_modified,
                    route,
                }) => Ok(Sync15ClientResponse::Success {
                    status: *status,
                    record: IncomingEncryptedBso::new(
                        record.envelope.clone(),
                        record.payload.clone(),
                    ),
                    last_modified: *last_modified,
                    route: route.clone(),
                }),
                // TODO(lina): Same as above, for 404s.
                _ => Ok(Sync15ClientResponse::Error(ErrorResponse::ServerError {
                    status: 500,
                    route: "test/path".into(),
                })),
            }
        }

        fn put_crypto_keys(
            &self,
            xius: ServerTimestamp,
            _keys: &OutgoingEncryptedBso,
        ) -> error::Result<()> {
            assert_eq!(xius, ServerTimestamp(888_800));
            Err(ErrorKind::StorageHttpError(ErrorResponse::ServerError {
                status: 500,
                route: "crypto/keys".to_string(),
            }))
        }

        fn wipe_all_remote(&self) -> error::Result<()> {
            Ok(())
        }
    }

    #[allow(clippy::unnecessary_wraps)]
    fn mocked_success_ts<T>(t: T, ts: i64) -> error::Result<Sync15ClientResponse<T>> {
        Ok(Sync15ClientResponse::Success {
            status: 200,
            record: t,
            last_modified: ServerTimestamp(ts),
            route: "test/path".into(),
        })
    }

    fn mocked_success<T>(t: T) -> error::Result<Sync15ClientResponse<T>> {
        mocked_success_ts(t, 0)
    }

    fn sync_ids_not_found() -> Sync15ClientResponse<Option<SyncIdsRecord>> {
        Sync15ClientResponse::Error(ErrorResponse::NotFound {
            route: "storage/meta/syncids".into(),
        })
    }

    fn mocked_success_keys(
        keys: CollectionKeys,
        root_key: &KeyBundle,
    ) -> error::Result<Sync15ClientResponse<IncomingEncryptedBso>> {
        let timestamp = keys.timestamp;
        let payload = keys.to_encrypted_payload(root_key).unwrap();
        let bso = IncomingEncryptedBso::new(
            IncomingEnvelope {
                id: Guid::new("keys"),
                modified: timestamp,
                sortindex: None,
                ttl: None,
            },
            payload,
        );
        Ok(Sync15ClientResponse::Success {
            status: 200,
            record: bso,
            last_modified: timestamp,
            route: "test/path".into(),
        })
    }

    #[test]
    fn test_state_machine_ready_from_empty() {
        nss_as::ensure_initialized();
        error_support::init_for_tests();
        let root_key = KeyBundle::new_random().unwrap();
        let keys = CollectionKeys {
            timestamp: ServerTimestamp(123_400),
            default: KeyBundle::new_random().unwrap(),
            collections: HashMap::new(),
        };
        let mg = MetaGlobalRecord {
            sync_id: "syncIDAAAAAA".into(),
            storage_version: 5usize,
            engines: vec![(
                "bookmarks",
                MetaGlobalEngine {
                    version: 1usize,
                    sync_id: "syncIDBBBBBB".into(),
                },
            )]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
            // We ensure that the record we upload doesn't have a logins record.
            declined: vec!["logins".to_string()],
        };
        let client = InMemoryClient {
            info_configuration: mocked_success(InfoConfiguration::default()),
            info_collections: mocked_success(InfoCollections::new(
                vec![("meta", 123_456), ("crypto", 145_000)]
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), ServerTimestamp(value)))
                    .collect(),
            )),
            meta_global: mocked_success_ts(mg, 999_000),
            crypto_keys: mocked_success_keys(keys, &root_key),
            sync_ids: sync_ids_not_found(),
            sync_ids_fetches: Cell::new(0),
            uploaded_globals: None,
        };
        let mut pgs = PersistedGlobalState::V2 { declined: None };

        let mut state_machine =
            SetupStateMachine::for_full_sync(&client, &root_key, &mut pgs, None, &NeverInterrupts);
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

    #[test]
    fn test_from_previous_state_declined() {
        nss_as::ensure_initialized();
        error_support::init_for_tests();
        // The state-machine sequence where we didn't use the previous state
        // (ie, where the state machine restarted)
        let sm_seq_restarted = vec![
            "WithPreviousState",
            "InitialWithConfig",
            "InitialWithInfo",
            "InitialWithMetaGlobal",
            "Ready",
        ];
        // The state-machine sequence where we used the previous state.
        let sm_seq_used_previous = vec!["WithPreviousState", "Ready"];

        // do the actual test.
        fn do_test(
            client: &dyn SetupStorageClient,
            root_key: &KeyBundle,
            pgs: &mut PersistedGlobalState,
            engine_updates: Option<&HashMap<String, bool>>,
            old_state: GlobalState,
            expected_states: &[&str],
        ) {
            let mut state_machine = SetupStateMachine::for_full_sync(
                client,
                root_key,
                pgs,
                engine_updates,
                &NeverInterrupts,
            );
            assert!(
                state_machine.run_to_ready(Some(old_state)).is_ok(),
                "Should drive state machine to ready"
            );
            assert_eq!(state_machine.sequence, expected_states);
        }

        // and all the complicated setup...
        let ts_metaglobal = 123_456;
        let ts_keys = 145_000;
        let root_key = KeyBundle::new_random().unwrap();
        let keys = CollectionKeys {
            timestamp: ServerTimestamp(ts_keys + 1),
            default: KeyBundle::new_random().unwrap(),
            collections: HashMap::new(),
        };
        let mg = MetaGlobalRecord {
            sync_id: "syncIDAAAAAA".into(),
            storage_version: 5usize,
            engines: vec![(
                "bookmarks",
                MetaGlobalEngine {
                    version: 1usize,
                    sync_id: "syncIDBBBBBB".into(),
                },
            )]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
            // We ensure that the record we upload doesn't have a logins record.
            declined: vec!["logins".to_string()],
        };
        let collections = InfoCollections::new(
            vec![("meta", ts_metaglobal), ("crypto", ts_keys)]
                .into_iter()
                .map(|(key, value)| (key.to_owned(), ServerTimestamp(value)))
                .collect(),
        );
        let client = InMemoryClient {
            info_configuration: mocked_success(InfoConfiguration::default()),
            info_collections: mocked_success(collections.clone()),
            meta_global: mocked_success_ts(mg.clone(), ts_metaglobal),
            crypto_keys: mocked_success_keys(keys.clone(), &root_key),
            sync_ids: sync_ids_not_found(),
            sync_ids_fetches: Cell::new(0),
            uploaded_globals: None,
        };

        // First a test where the "previous" global state is OK to reuse.
        {
            let mut pgs = PersistedGlobalState::V2 { declined: None };
            // A "previous" global state.
            let old_state = GlobalState {
                config: InfoConfiguration::default(),
                collections: collections.clone(),
                global: mg.clone(),
                global_timestamp: ServerTimestamp(ts_metaglobal),
                meta_timestamp: ServerTimestamp(ts_metaglobal),
                keys: keys
                    .to_encrypted_payload(&root_key)
                    .expect("should always work in this test"),
                keys_timestamp: ServerTimestamp(ts_keys),
            };
            do_test(
                &client,
                &root_key,
                &mut pgs,
                None,
                old_state,
                &sm_seq_used_previous,
            );
        }

        // Now where the meta/global record on the server is later.
        {
            let mut pgs = PersistedGlobalState::V2 { declined: None };
            // A "previous" global state.
            let old_state = GlobalState {
                config: InfoConfiguration::default(),
                collections: collections.clone(),
                global: mg.clone(),
                global_timestamp: ServerTimestamp(999_999),
                meta_timestamp: ServerTimestamp(999_999),
                keys: keys
                    .to_encrypted_payload(&root_key)
                    .expect("should always work in this test"),
                keys_timestamp: ServerTimestamp(ts_keys),
            };
            do_test(
                &client,
                &root_key,
                &mut pgs,
                None,
                old_state,
                &sm_seq_restarted,
            );
        }

        // Where meta/global is older than the `meta` collection, because some
        // other record in it (eg, meta/syncids) was written later. We last saw
        // the collection at the same timestamp, so nothing has changed.
        {
            let mut pgs = PersistedGlobalState::V2 { declined: None };
            let old_state = GlobalState {
                config: InfoConfiguration::default(),
                collections: collections.clone(),
                global: mg.clone(),
                global_timestamp: ServerTimestamp(ts_metaglobal - 1000),
                meta_timestamp: ServerTimestamp(ts_metaglobal),
                keys: keys
                    .to_encrypted_payload(&root_key)
                    .expect("should always work in this test"),
                keys_timestamp: ServerTimestamp(ts_keys),
            };
            do_test(
                &client,
                &root_key,
                &mut pgs,
                None,
                old_state,
                &sm_seq_used_previous,
            );
        }

        // Where keys on the server is later.
        {
            let mut pgs = PersistedGlobalState::V2 { declined: None };
            // A "previous" global state.
            let old_state = GlobalState {
                config: InfoConfiguration::default(),
                collections: collections.clone(),
                global: mg.clone(),
                global_timestamp: ServerTimestamp(ts_metaglobal),
                meta_timestamp: ServerTimestamp(ts_metaglobal),
                keys: keys
                    .to_encrypted_payload(&root_key)
                    .expect("should always work in this test"),
                keys_timestamp: ServerTimestamp(999_999),
            };
            do_test(
                &client,
                &root_key,
                &mut pgs,
                None,
                old_state,
                &sm_seq_restarted,
            );
        }

        // Where there are engine-state changes.
        {
            let mut pgs = PersistedGlobalState::V2 { declined: None };
            // A "previous" global state.
            let old_state = GlobalState {
                config: InfoConfiguration::default(),
                collections,
                global: mg,
                global_timestamp: ServerTimestamp(ts_metaglobal),
                meta_timestamp: ServerTimestamp(ts_metaglobal),
                keys: keys
                    .to_encrypted_payload(&root_key)
                    .expect("should always work in this test"),
                keys_timestamp: ServerTimestamp(ts_keys),
            };
            let mut engine_updates = HashMap::<String, bool>::new();
            engine_updates.insert("logins".to_string(), false);
            do_test(
                &client,
                &root_key,
                &mut pgs,
                Some(&engine_updates),
                old_state,
                &sm_seq_restarted,
            );
            let declined = match pgs {
                PersistedGlobalState::V2 { declined: d } => d,
            };
            // and check we now consider logins as declined.
            assert_eq!(declined, Some(vec!["logins".to_string()]));
        }
    }

    fn string_set(s: &[&str]) -> HashSet<String> {
        s.iter().map(ToString::to_string).collect()
    }
    fn string_map<T: Clone>(s: &[(&str, T)]) -> HashMap<String, T> {
        s.iter().map(|v| (v.0.to_string(), v.1.clone())).collect()
    }
    #[test]
    fn test_engine_states() {
        assert_eq!(
            compute_engine_states(EngineStateInput {
                local_declined: string_set(&["foo", "bar"]),
                remote: None,
                user_changes: Default::default(),
            }),
            EngineStateOutput {
                declined: string_set(&["foo", "bar"]),
                // No wipes, no resets
                changes_needed: Default::default(),
            }
        );
        assert_eq!(
            compute_engine_states(EngineStateInput {
                local_declined: string_set(&["foo", "bar"]),
                remote: Some(RemoteEngineState {
                    declined: string_set(&["foo"]),
                    info_collections: string_set(&["bar"])
                }),
                user_changes: Default::default(),
            }),
            EngineStateOutput {
                // Now we have `foo`.
                declined: string_set(&["foo"]),
                // No wipes, no resets, should just be a local update.
                changes_needed: Default::default(),
            }
        );
        assert_eq!(
            compute_engine_states(EngineStateInput {
                local_declined: string_set(&["foo", "bar"]),
                remote: Some(RemoteEngineState {
                    declined: string_set(&["foo", "bar", "quux"]),
                    info_collections: string_set(&[])
                }),
                user_changes: Default::default(),
            }),
            EngineStateOutput {
                // Now we have `foo`.
                declined: string_set(&["foo", "bar", "quux"]),
                changes_needed: EngineChangesNeeded {
                    // Should reset `quux`.
                    local_resets: string_set(&["quux"]),
                    // No wipes, though.
                    remote_wipes: string_set(&[]),
                }
            }
        );
        assert_eq!(
            compute_engine_states(EngineStateInput {
                local_declined: string_set(&["bar", "baz"]),
                remote: Some(RemoteEngineState {
                    declined: string_set(&["bar", "baz",]),
                    info_collections: string_set(&["quux"])
                }),
                // Change a declined engine to undeclined.
                user_changes: string_map(&[("bar", true)]),
            }),
            EngineStateOutput {
                declined: string_set(&["baz"]),
                // No wipes, just undecline it.
                changes_needed: Default::default()
            }
        );
        assert_eq!(
            compute_engine_states(EngineStateInput {
                local_declined: string_set(&["bar", "baz"]),
                remote: Some(RemoteEngineState {
                    declined: string_set(&["bar", "baz"]),
                    info_collections: string_set(&["foo"])
                }),
                // Change an engine which exists remotely to declined.
                user_changes: string_map(&[("foo", false)]),
            }),
            EngineStateOutput {
                declined: string_set(&["baz", "bar", "foo"]),
                // No wipes, just undecline it.
                changes_needed: EngineChangesNeeded {
                    // Should reset our local foo
                    local_resets: string_set(&["foo"]),
                    // And wipe the server.
                    remote_wipes: string_set(&["foo"]),
                }
            }
        );
    }

    const GLOBAL_SYNC_ID: &str = "syncIDAAAAAA";
    const META_TS: i64 = 200_000;
    const GLOBAL_TS: i64 = 150_000;

    fn meta_global(engines: &[(&str, usize, &str)], declined: &[&str]) -> MetaGlobalRecord {
        MetaGlobalRecord {
            sync_id: GLOBAL_SYNC_ID.into(),
            storage_version: STORAGE_VERSION,
            engines: engines
                .iter()
                .map(|(name, version, sync_id)| {
                    (
                        name.to_string(),
                        MetaGlobalEngine {
                            version: *version,
                            sync_id: (*sync_id).into(),
                        },
                    )
                })
                .collect(),
            declined: declined.iter().map(ToString::to_string).collect(),
        }
    }

    /// A meta/global with every default engine, except those listed.
    fn meta_global_without(missing: &[&str], declined: &[&str]) -> MetaGlobalRecord {
        let engines: Vec<_> = DEFAULT_ENGINES
            .iter()
            .filter(|(name, _)| !missing.contains(name) && !declined.contains(name))
            .map(|(name, version)| (*name, *version, "syncIDGLOBAL"))
            .collect();
        meta_global(&engines, declined)
    }

    fn sync_ids(engines: &[(&str, usize, &str)]) -> Sync15ClientResponse<Option<SyncIdsRecord>> {
        let record = SyncIdsRecord {
            version: 1,
            global_sync_id: Some(GLOBAL_SYNC_ID.into()),
            engines: meta_global(engines, &[]).engines,
        };
        Sync15ClientResponse::Success {
            status: 200,
            record: Some(record),
            last_modified: ServerTimestamp(META_TS),
            route: "storage/meta/syncids".into(),
        }
    }

    fn recording_client(
        mg: MetaGlobalRecord,
        sync_ids: Sync15ClientResponse<Option<SyncIdsRecord>>,
        root_key: &KeyBundle,
    ) -> InMemoryClient {
        let keys = CollectionKeys {
            timestamp: ServerTimestamp(145_000),
            default: KeyBundle::new_random().unwrap(),
            collections: HashMap::new(),
        };
        InMemoryClient {
            info_configuration: mocked_success(InfoConfiguration::default()),
            info_collections: mocked_success(InfoCollections::new(
                vec![("meta", META_TS), ("crypto", 145_000)]
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), ServerTimestamp(value)))
                    .collect(),
            )),
            meta_global: mocked_success_ts(mg, GLOBAL_TS),
            crypto_keys: mocked_success_keys(keys, root_key),
            sync_ids,
            sync_ids_fetches: Cell::new(0),
            uploaded_globals: Some(RefCell::new(vec![])),
        }
    }

    fn run(
        client: &InMemoryClient,
        root_key: &KeyBundle,
        pgs: &mut PersistedGlobalState,
    ) -> error::Result<GlobalState> {
        SetupStateMachine::for_full_sync(client, root_key, pgs, None, &NeverInterrupts)
            .run_to_ready(None)
    }

    fn uploaded(client: &InMemoryClient) -> Vec<MetaGlobalRecord> {
        client.uploaded_globals.as_ref().unwrap().borrow().clone()
    }

    fn pgs_declined(pgs: &PersistedGlobalState) -> Option<HashSet<String>> {
        match pgs {
            PersistedGlobalState::V2 { declined } => {
                declined.as_ref().map(|d| d.iter().cloned().collect())
            }
        }
    }

    fn engine_names(global: &MetaGlobalRecord) -> HashSet<String> {
        global.engines.keys().cloned().collect()
    }

    fn default_engines_except(except: &[&str]) -> HashSet<String> {
        DEFAULT_ENGINES
            .iter()
            .map(|(name, _)| name.to_string())
            .filter(|name| !except.contains(&name.as_str()))
            .collect()
    }

    #[test]
    fn test_default_engine_versions() {
        // These must match the versions Desktop uses, otherwise we'll never
        // adopt sync IDs from meta/syncids and will force engine resets.
        // (Desktop's creditcards engine is version 1, even though its schema
        // version is not.)
        assert_eq!(
            DEFAULT_ENGINES,
            &[
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
            ]
        );
    }

    #[test]
    fn test_new_global_omits_declined() {
        let pgs = PersistedGlobalState::V2 {
            declined: Some(vec!["passwords".to_string()]),
        };
        let global = new_global(&pgs);
        assert_eq!(
            engine_names(&global),
            default_engines_except(&["passwords"])
        );
        assert_eq!(global.declined, vec!["passwords".to_string()]);

        let global = new_global(&PersistedGlobalState::default());
        assert_eq!(engine_names(&global), default_engines_except(&[]));
        assert!(global.declined.is_empty());
    }

    #[test]
    fn test_fixup_adopts_sync_ids() {
        nss_as::ensure_initialized();
        error_support::init_for_tests();
        let root_key = KeyBundle::new_random().unwrap();
        let client = recording_client(
            meta_global_without(&["history", "tabs", "clients"], &[]),
            sync_ids(&[
                ("history", 1, "historyREGID"),
                ("clients", 1, "clientsREGID"),
                // wrong version, so can't be adopted.
                ("tabs", 2, "tabsREGIDAAA"),
                // already in meta/global, which wins.
                ("bookmarks", 2, "bookmarksREG"),
            ]),
            &root_key,
        );
        let mut pgs = PersistedGlobalState::V2 {
            declined: Some(vec![]),
        };
        let state = run(&client, &root_key, &mut pgs).expect("should work");
        assert_eq!(client.sync_ids_fetches.get(), 1);
        let uploaded = uploaded(&client);
        assert_eq!(uploaded.len(), 1);
        let engines = &uploaded[0].engines;
        assert_eq!(engines["history"].sync_id, "historyREGID");
        assert_eq!(engines["clients"].sync_id, "clientsREGID");
        assert_ne!(engines["tabs"].sync_id, "tabsREGIDAAA");
        assert_eq!(engines["tabs"].version, 1);
        assert_eq!(engines["bookmarks"].sync_id, "syncIDGLOBAL");
        // We wrote meta/global, so that's now the latest in the `meta` collection.
        assert_eq!(state.meta_timestamp, ServerTimestamp(GLOBAL_TS + 1));
    }

    #[test]
    fn test_fixup_ignores_sync_ids_for_another_global() {
        nss_as::ensure_initialized();
        error_support::init_for_tests();
        let root_key = KeyBundle::new_random().unwrap();
        let mut response = sync_ids(&[("history", 1, "historyREGID")]);
        if let Sync15ClientResponse::Success {
            record: Some(r), ..
        } = &mut response
        {
            r.global_sync_id = Some("syncIDOTHERR".into());
        }
        let client = recording_client(meta_global_without(&["history"], &[]), response, &root_key);
        let mut pgs = PersistedGlobalState::V2 {
            declined: Some(vec![]),
        };
        run(&client, &root_key, &mut pgs).expect("should work");
        assert_ne!(
            uploaded(&client)[0].engines["history"].sync_id,
            "historyREGID"
        );
    }

    #[test]
    fn test_fixup_without_sync_ids() {
        nss_as::ensure_initialized();
        error_support::init_for_tests();
        let root_key = KeyBundle::new_random().unwrap();
        let unparseable = Sync15ClientResponse::Success {
            status: 200,
            record: None,
            last_modified: ServerTimestamp(META_TS),
            route: "storage/meta/syncids".into(),
        };
        for response in [sync_ids_not_found(), unparseable] {
            let client =
                recording_client(meta_global_without(&["history"], &[]), response, &root_key);
            let mut pgs = PersistedGlobalState::V2 {
                declined: Some(vec![]),
            };
            run(&client, &root_key, &mut pgs).expect("should mint as before");
            let uploaded = uploaded(&client);
            assert_eq!(uploaded.len(), 1);
            assert_eq!(uploaded[0].engines["history"].version, 1);
        }
    }

    #[test]
    fn test_fixup_sync_ids_error() {
        nss_as::ensure_initialized();
        error_support::init_for_tests();
        let root_key = KeyBundle::new_random().unwrap();
        let route = "storage/meta/syncids".to_string();
        for error in [
            ErrorResponse::ServerError {
                route: route.clone(),
                status: 503,
            },
            ErrorResponse::Unauthorized {
                route: route.clone(),
            },
        ] {
            let client = recording_client(
                meta_global_without(&["history"], &[]),
                Sync15ClientResponse::Error(error),
                &root_key,
            );
            // A user change, which must not be persisted when we fail.
            let mut changes = HashMap::new();
            changes.insert("tabs".to_string(), false);
            let mut pgs = PersistedGlobalState::V2 {
                declined: Some(vec!["passwords".to_string()]),
            };
            let mut sm = SetupStateMachine::for_full_sync(
                &client,
                &root_key,
                &mut pgs,
                Some(&changes),
                &NeverInterrupts,
            );
            assert!(sm.run_to_ready(None).is_err());
            assert!(sm.changes_needed.is_none());
            assert!(uploaded(&client).is_empty(), "nothing should be minted");
            assert_eq!(pgs_declined(&pgs), Some(string_set(&["passwords"])));
        }
    }

    #[test]
    fn test_nothing_missing_skips_sync_ids() {
        nss_as::ensure_initialized();
        error_support::init_for_tests();
        let root_key = KeyBundle::new_random().unwrap();
        let client = recording_client(
            meta_global_without(&[], &["passwords"]),
            // would fail the sync if we fetched it.
            Sync15ClientResponse::Error(ErrorResponse::ServerError {
                route: "storage/meta/syncids".into(),
                status: 500,
            }),
            &root_key,
        );
        let mut pgs = PersistedGlobalState::V2 {
            declined: Some(vec!["passwords".to_string()]),
        };
        let state = run(&client, &root_key, &mut pgs).expect("should work");
        assert_eq!(client.sync_ids_fetches.get(), 0);
        assert!(uploaded(&client).is_empty());
        // We didn't write anything, so we've seen `meta` as of info/collections,
        // which is later than meta/global itself.
        assert_eq!(state.meta_timestamp, ServerTimestamp(META_TS));
        assert_eq!(state.global_timestamp, ServerTimestamp(GLOBAL_TS));

        // So next time we can use that state as-is, even though meta/global
        // is older than the `meta` collection.
        let mut sm =
            SetupStateMachine::for_full_sync(&client, &root_key, &mut pgs, None, &NeverInterrupts);
        sm.run_to_ready(Some(state)).expect("should work");
        assert_eq!(sm.sequence, vec!["WithPreviousState", "Ready"]);
    }

    #[test]
    fn test_empty_meta_global_keeps_local_declined() {
        nss_as::ensure_initialized();
        error_support::init_for_tests();
        let root_key = KeyBundle::new_random().unwrap();
        // What a client using per-device engine choices writes.
        let client = recording_client(
            meta_global(&[], &[]),
            sync_ids(&[
                ("bookmarks", 2, "bookmarksREG"),
                ("passwords", 1, "passwordsREG"),
            ]),
            &root_key,
        );
        let mut pgs = PersistedGlobalState::V2 {
            declined: Some(vec!["passwords".to_string(), "history".to_string()]),
        };
        let mut sm =
            SetupStateMachine::for_full_sync(&client, &root_key, &mut pgs, None, &NeverInterrupts);
        sm.run_to_ready(None).expect("should work");
        assert_eq!(sm.changes_needed, Some(EngineChangesNeeded::default()));
        assert_eq!(
            pgs_declined(&pgs),
            Some(string_set(&["passwords", "history"]))
        );
        let uploaded = uploaded(&client);
        assert_eq!(uploaded.len(), 1);
        let global = &uploaded[0];
        assert_eq!(
            global.declined.iter().cloned().collect::<HashSet<_>>(),
            string_set(&["passwords", "history"])
        );
        assert_eq!(
            engine_names(global),
            default_engines_except(&["passwords", "history"])
        );
        assert_eq!(global.engines["bookmarks"].sync_id, "bookmarksREG");
    }

    #[test]
    fn test_empty_engines_with_declined_is_legacy() {
        nss_as::ensure_initialized();
        error_support::init_for_tests();
        let root_key = KeyBundle::new_random().unwrap();
        let client = recording_client(
            meta_global(&[], &["history"]),
            sync_ids_not_found(),
            &root_key,
        );
        let mut pgs = PersistedGlobalState::V2 {
            declined: Some(vec!["passwords".to_string()]),
        };
        run(&client, &root_key, &mut pgs).expect("should work");
        // We adopted the remote declined list, as usual.
        assert_eq!(pgs_declined(&pgs), Some(string_set(&["history"])));
        let uploaded = uploaded(&client);
        assert_eq!(uploaded[0].declined, vec!["history".to_string()]);
        assert_eq!(
            engine_names(&uploaded[0]),
            default_engines_except(&["history"])
        );
    }

    #[test]
    fn test_empty_meta_global_unknown_declined() {
        nss_as::ensure_initialized();
        error_support::init_for_tests();
        let root_key = KeyBundle::new_random().unwrap();
        let client = recording_client(meta_global(&[], &[]), sync_ids_not_found(), &root_key);
        // We've no idea what's declined, so everything ends up enabled, as
        // happens today.
        let mut pgs = PersistedGlobalState::default();
        run(&client, &root_key, &mut pgs).expect("should work");
        assert_eq!(pgs_declined(&pgs), Some(HashSet::new()));
        let uploaded = uploaded(&client);
        assert!(uploaded[0].declined.is_empty());
        assert_eq!(engine_names(&uploaded[0]), default_engines_except(&[]));
    }

    #[test]
    fn test_everything_declined_is_legacy() {
        nss_as::ensure_initialized();
        error_support::init_for_tests();
        let root_key = KeyBundle::new_random().unwrap();
        // What Desktop writes when everything is disabled - clients remains.
        let declined: Vec<&str> = DEFAULT_ENGINES
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| *name != "clients")
            .collect();
        let client = recording_client(
            meta_global(&[("clients", 1, "clientsGLOBAL")], &declined),
            sync_ids_not_found(),
            &root_key,
        );
        let mut pgs = PersistedGlobalState::V2 {
            declined: Some(vec![]),
        };
        run(&client, &root_key, &mut pgs).expect("should work");
        assert_eq!(pgs_declined(&pgs), Some(string_set(&declined)));
        assert!(uploaded(&client).is_empty());
        assert_eq!(client.sync_ids_fetches.get(), 0);
    }
}

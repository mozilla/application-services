/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// This helps you perform a sync of multiple stores and helps you manage
// global and local state between syncs.

use crate::client::{Sync15StorageClient, Sync15StorageClientInit};
use crate::error::Error;
use crate::key_bundle::KeyBundle;
use crate::state::{GlobalState, PersistedGlobalState, SetupStateMachine};
use crate::status::ServiceStatus;
use crate::sync::{self, Store};
use crate::telemetry;
use interrupt::Interruptee;
use std::collections::HashMap;
use std::mem;
use std::result;

/// Info about the client to use. We reuse the client unless
/// we discover the client_init has changed, in which case we re-create one.
#[derive(Debug)]
struct ClientInfo {
    // the client_init used to create `client`.
    client_init: Sync15StorageClientInit,
    // the client (our tokenserver state machine state, and our http library's state)
    client: Sync15StorageClient,
}

/// Info we want callers to store *in memory* for us so that subsequent
/// syncs are faster. This should never be persisted to storage as it holds
/// sensitive information, such as the sync decryption keys.
#[derive(Debug, Default)]
pub struct MemoryCachedState {
    last_client_info: Option<ClientInfo>,
    last_global_state: Option<GlobalState>,
}

/// Sync multiple stores
/// * `stores` - The stores to sync
/// * `persisted_global_state` - The global state to use, or None if never
///   before provided. At the end of the sync, and even when the sync fails,
///   the value in this cell should be persisted to permanent storage and
///   provided next time the sync is called.
/// * `last_client_info` - The client state to use, or None if never before
///   provided. At the end of the sync, the value should be persisted
///   *in memory only* - it should not be persisted to disk.
/// * `storage_init` - Information about how the sync http client should be
///   configured.
/// * `root_sync_key` - The KeyBundle used for encryption.
///
/// Returns a map, keyed by name and holding an error value - if any store
/// fails, the sync will continue on to other stores, but the error will be
/// places in this map. The absence of a name in the map implies the store
/// succeeded.
pub fn sync_multiple(
    stores: &[&dyn Store],
    persisted_global_state: &mut Option<String>,
    mem_cached_state: &mut MemoryCachedState,
    storage_init: &Sync15StorageClientInit,
    root_sync_key: &KeyBundle,
    sync_ping: &mut telemetry::SyncTelemetryPing,
    interruptee: &impl Interruptee,
    service_status: &mut ServiceStatus,
) -> result::Result<HashMap<String, Error>, Error> {
    *service_status = ServiceStatus::OtherError; // we'll set this to better values as we go.

    if let Err(e) = interruptee.err_if_interrupted() {
        *service_status = ServiceStatus::Interrupted;
        return Err(e.into());
    }
    let mut pgs = match persisted_global_state {
        Some(persisted_string) => {
            match serde_json::from_str::<PersistedGlobalState>(&persisted_string) {
                Ok(state) => state,
                _ => {
                    // Don't log the error since it might contain sensitive
                    // info (although currently it only contains the declined engines list)
                    log::error!(
                        "Failed to parse PersistedGlobalState from JSON! Falling back to default"
                    );
                    PersistedGlobalState::default()
                }
            }
        }
        None => {
            log::warn!("The application didn't give us persisted state - this is only expected on the very first run");
            PersistedGlobalState::default()
        }
    };

    // We put None back into last_client_info now so if we fail entirely,
    // reinitialize everything related to the client.
    let client_info = match mem::replace(&mut mem_cached_state.last_client_info, None) {
        Some(client_info) => {
            // if our storage_init has changed we can't reuse the client
            if client_info.client_init != *storage_init {
                ClientInfo {
                    client_init: storage_init.clone(),
                    client: Sync15StorageClient::new(storage_init.clone())?,
                }
            } else {
                // we can reuse it (which should be the common path)
                client_info
            }
        }
        None => ClientInfo {
            client_init: storage_init.clone(),
            client: Sync15StorageClient::new(storage_init.clone())?,
        },
    };
    if let Err(e) = interruptee.err_if_interrupted() {
        *service_status = ServiceStatus::Interrupted;
        return Err(e.into());
    }

    // Advance the state machine to the point where it can perform a full
    // sync. This may involve uploading meta/global, crypto/keys etc.
    let global_state = {
        let last_state = mem::replace(&mut mem_cached_state.last_global_state, None);
        let mut state_machine = SetupStateMachine::for_full_sync(
            &client_info.client,
            &root_sync_key,
            &mut pgs,
            interruptee,
        );
        log::info!("Advancing state machine to ready (full)");
        let state = match state_machine.run_to_ready(last_state) {
            Err(e) => {
                *service_status = ServiceStatus::from_err(&e);
                return Err(e.into());
            }
            Ok(state) => state,
        };
        // The state machine might have updated our persisted_global_state, so
        // update the callers repr of it.
        mem::replace(persisted_global_state, Some(serde_json::to_string(&pgs)?));
        sync_ping.uid(client_info.client.hashed_uid()?);
        // As for client_info, put None back now so we start from scratch on error.
        mem_cached_state.last_global_state = None;
        state
    };

    // Set the service status to OK here - we may adjust it based on an individual
    // store failing.
    *service_status = ServiceStatus::Ok;

    let mut telem_sync = telemetry::SyncTelemetry::new();
    let mut failures: HashMap<String, Error> = HashMap::new();
    for store in stores {
        let name = store.collection_name();
        log::info!("Syncing {} engine!", name);

        let mut telem_engine = telemetry::Engine::new(name);
        let result = sync::synchronize(
            &client_info.client,
            &global_state,
            *store,
            true,
            &mut telem_engine,
            interruptee,
        );

        match result {
            Ok(()) => log::info!("Sync of {} was successful!", name),
            Err(e) => {
                // XXX - while we arrange to reset the global state machine
                // here via, ideally we'd be more fine-grained
                // about it - eg, a simple network error shouldn't cause this.
                // However, the costs of restarting the state machine from
                // scratch really isn't that bad for now.
                log::warn!("Sync of {} failed! {:?}", name, e);
                let this_status = ServiceStatus::from_err(&e);
                let f = telemetry::sync_failure_from_error(&e);
                failures.insert(name.into(), e);
                telem_engine.failure(f);
                // If the failure from the store looks like anything other than
                // a "store error" we don't bother trying the others.
                if this_status != ServiceStatus::OtherError {
                    *service_status = this_status;
                    break;
                }
            }
        }
        telem_sync.engine(telem_engine);
        if let Err(e) = interruptee.err_if_interrupted() {
            *service_status = ServiceStatus::Interrupted;
            return Err(e.into());
        }
    }

    sync_ping.sync(telem_sync);
    if !failures.is_empty() {
        log::info!("Updating persisted global state");
        mem_cached_state.last_client_info = Some(client_info);
        mem_cached_state.last_global_state = Some(global_state);
    }

    Ok(failures)
}

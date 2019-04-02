/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// This helps you perform a sync of multiple stores and helps you manage
// global and local state between syncs.

use crate::client::{Sync15StorageClient, Sync15StorageClientInit};
use crate::error::Error;
use crate::key_bundle::KeyBundle;
use crate::state::{ApplicationState, GlobalState, SetupStateMachine};
use crate::sync::{self, Store};
use crate::telemetry;
use std::cell::Cell;
use std::collections::HashMap;
use std::result;

/// Info stored in memory about the client to use. We reuse the client unless
/// we discover the client_init has changed, in which case we re-create one.
#[derive(Debug)]
pub struct ClientInfo {
    // the client_init used to create `client`.
    client_init: Sync15StorageClientInit,
    // the client (our tokenserver state machine state, and our http library's state)
    client: Sync15StorageClient,
}

impl ClientInfo {
    /// Get the `Sync15StorageClient`. Only visible for testing.
    pub fn test_only_get_client(&self) -> &Sync15StorageClient {
        &self.client
    }
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
    maybe_global_state: &Cell<Option<GlobalState>>,
    last_client_info: &Cell<Option<ClientInfo>>,
    storage_init: &Sync15StorageClientInit,
    root_sync_key: &KeyBundle,
    sync_ping: &mut telemetry::SyncTelemetryPing,
) -> result::Result<HashMap<String, Error>, Error> {
    // We swap None for the ClientInfo, so if we fail below the cell will have
    // None causing us to be re-initialized on the next sync.
    let client_info = match last_client_info.replace(None) {
        Some(client_info) => {
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

    // Advance the state machine to the point where it can perform a full
    // sync. This may involve uploading meta/global, crypto/keys etc.
    let global_state = {
        // Scope borrow of `sync_info.client`
        let existing = maybe_global_state.replace(None);
        let app_state = ApplicationState::default(); // XXX - see comments for ApplicationState

        let mut state_machine =
            SetupStateMachine::for_full_sync(&client_info.client, &root_sync_key, &app_state);
        log::info!("Advancing state machine to ready (full)");
        let state = state_machine.run_to_ready(existing)?;
        sync_ping.uid(client_info.client.hashed_uid()?);
        state
    };

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
        );

        match result {
            Ok(()) => log::info!("Sync of {} was successful!", name),
            Err(e) => {
                // XXX - should we wipe the global state here? Ideally we'd
                // be able to tell a "store error" vs a "state error".
                // (OTOH, a "state error" for every engine probably doesn't
                // really hurt, and we'll resolve it next time)
                log::warn!("Sync of {} failed! {:?}", name, e);
                let f = telemetry::sync_failure_from_error(&e);
                failures.insert(name.into(), e);
                telem_engine.failure(f);
            }
        }
        telem_sync.engine(telem_engine);
    }

    sync_ping.sync(telem_sync);
    log::info!("Updating persisted global state");
    maybe_global_state.replace(Some(global_state));
    last_client_info.replace(Some(client_info));

    Ok(failures)
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// traits etc to drive a "full sync".

use std::result;
use failure;
use client::{Sync15StorageClient, Sync15StorageClientInit};
use state::{GlobalState, SetupStateMachine};
use sync::{self, Store};
use key_bundle::KeyBundle;
use error::Error;

// Info stored in memory about the client to use. We reuse the client unless
// we discover the client_init has changed, in which case we re-create one.
pub struct ClientInfo {
    // the client_init used to create the client.
    client_init: Sync15StorageClientInit,
    // the client we will reuse if possible.
    client: Sync15StorageClient,
}

pub trait GlobalStateProvider {
    fn load(&self) -> result::Result<Option<GlobalState>, failure::Error>;

    fn save(&self, state: Option<&GlobalState>) -> result::Result<(), failure::Error>;

    // A ClientInfo is stored in memory with ownership transfered to and from
    // the GlobalStateProvider by way of this function.
    // XXX - it's not really clear that this should be on the GlobalStateProvider,
    // especially given it's not persisted anywhere.
    fn swap_client_info(&self, Option<ClientInfo>) -> result::Result<Option<ClientInfo>, failure::Error>;
}

pub fn sync_global(
    store: &Store,
    gsp: &GlobalStateProvider,
    storage_init: &Sync15StorageClientInit,
    root_sync_key: &KeyBundle
) -> result::Result<(), Error> {
    let maybe_global = gsp.load()?;
    // Note: We explicitly write a None back as the state, meaning if we
    // unexpectedly fail below, the next sync will redownload meta/global,
    // crypto/keys, etc. without needing to. Apparently this is both okay
    // and by design.
    gsp.save(None)?;
    let mut global_state = match maybe_global {
        Some(g) => g,
        None => {
            info!("First time through since unlock. Creating default global state.");
            gsp.swap_client_info(None)?;
            GlobalState::default()
        }
    };

    // Ditto for the ClientInfo - if we fail below the GlobalStateProvider will
    // not have the last client and client_init, so will be re-initialized on
    // the next sync.
    let client_info = match gsp.swap_client_info(None)? {
        Some(client_info) => {
            if client_info.client_init != *storage_init {
                ClientInfo {
                    client_init: storage_init.clone(),
                    client: Sync15StorageClient::new(storage_init.clone())?,
                }
            } else {
                // we can reuse it.
                client_info
            }
        },
        None => {
            ClientInfo {
                client_init: storage_init.clone(),
                client: Sync15StorageClient::new(storage_init.clone())?,
            }
        }
    };

    // Advance the state machine to the point where it can perform a full
    // sync. This may involve uploading meta/global, crypto/keys etc.
    {
        // Scope borrow of `sync_info.client`
        let mut state_machine =
            SetupStateMachine::for_full_sync(&client_info.client, &root_sync_key);
        info!("Advancing state machine to ready (full)");
        global_state = state_machine.to_ready(global_state)?;
    }

    // Reset our local state if necessary.
    if global_state.engines_that_need_local_reset().contains("passwords") {
        info!("Passwords sync ID changed; engine needs local reset");
        store.reset()?;
    }

    // Persist the current sync state in the DB.
    info!("Updating persisted global state");
    gsp.save(Some(&global_state))?;

    info!("Syncing passwords engine!");
    let ts = store.get_last_sync()?.unwrap_or_default();

    // We don't use `?` here so that we can restore the value of of
    // `self.sync` even if sync fails.
    let result = sync::synchronize(
        &client_info.client,
        &global_state,
        store,
        "passwords".into(),
        ts,
        true
    );

    match &result {
        Ok(()) => info!("Sync was successful!"),
        Err(e) => warn!("Sync failed! {:?}", e),
    }

    // Tell the global state provider about the new client info.
    gsp.swap_client_info(Some(client_info))?;

    Ok(result?)
}


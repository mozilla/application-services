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


pub trait GlobalStateProvider {
    fn load(&self) -> result::Result<Option<GlobalState>, failure::Error>;

    fn save(&self, state: Option<&GlobalState>) -> result::Result<(), failure::Error>;

    // Store in memory, but do not persist, a Sync15StorageClientInit
    fn get_client_init(&self) -> result::Result<Option<Sync15StorageClientInit>, failure::Error>;

    fn set_client_init(&self, init: Option<Sync15StorageClientInit>) -> result::Result<(), failure::Error>;
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
    // `maybe_sync_info` is None if we haven't called `sync` since
    // restarting the browser.
    //
    // If this is the case we may or may not have a persisted version of
    // GlobalState stored in the DB (we will iff we've synced before, unless
    // we've `reset()`, which clears it out).
    // XXX - fix comment above?

    let mut global_state = match maybe_global {
        Some(g) => g,
        None => {
            info!("First time through since unlock. Creating default global state.");
            gsp.set_client_init(None)?;
            GlobalState::default()
        }
    };

    // XXX - storage_init.
//            };
        let client = Sync15StorageClient::new(storage_init.clone())?;
    //     Ok(SyncInfo {
    //         state,
    //         client,
    //         last_client_init: storage_init.clone(),
    //     })
    // })?;

    // If the options passed for initialization of the storage client aren't
    // the same as the ones we used last time, reinitialize it. (Note that
    // we could avoid the comparison in the case where we had `None` in
    // `state.sync` before, but this probably doesn't matter).
    //
    // It's a little confusing that we do things this way (transparently
    // re-initialize the client), but it reduces the size of the API surface
    // exposed over the FFI, and simplifies the states that the client code
    // has to consider (as far as it's concerned it just has to pass
    // `current` values for these things, and not worry about having to
    // re-initialize the sync state).
/*        
    if storage_init != &sync_info.last_client_init {
        info!("Detected change in storage client init, updating");
        sync_info.client = Sync15StorageClient::new(storage_init.clone())?;
        sync_info.last_client_init = storage_init.clone();
    }
*/
    // Advance the state machine to the point where it can perform a full
    // sync. This may involve uploading meta/global, crypto/keys etc.
    {
        // Scope borrow of `sync_info.client`
        let mut state_machine =
            SetupStateMachine::for_full_sync(&client, &root_sync_key);
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
        &client,
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

    // Restore our value of `sync_info` even if the sync failed.
//        self.sync = Some(sync_info);

    Ok(result?)
}


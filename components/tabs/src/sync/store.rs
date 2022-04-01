/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use crate::storage::{ClientRemoteTabs, RemoteTab, TabsStorage};
use crate::sync::engine::TabsEngine;
use interrupt_support::NeverInterrupts;
use std::cell::RefCell;
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};
use sync15::{
    sync_multiple, telemetry, KeyBundle, MemoryCachedState, Sync15StorageClientInit, SyncEngine,
    SyncEngineId,
};

// Our "sync manager" will use whatever is stashed here.
lazy_static::lazy_static! {
    // Mutex: just taken long enough to update the inner stuff
    static ref STORE_FOR_MANAGER: Mutex<Weak<TabsStore>> = Mutex::new(Weak::new());
}

/// Called by the sync manager to get a sync engine via the store previously
/// registered with the sync manager.
pub fn get_registered_sync_engine(engine_id: &SyncEngineId) -> Option<Box<dyn SyncEngine>> {
    let weak = STORE_FOR_MANAGER.lock().unwrap();
    match weak.upgrade() {
        None => None,
        Some(store) => match engine_id {
            SyncEngineId::Tabs => Some(Box::new(TabsEngine::new(Arc::clone(&store)))),
            // panicing here seems reasonable - it's a static error if this
            // it hit, not something that runtime conditions can influence.
            _ => unreachable!("can't provide unknown engine: {}", engine_id),
        },
    }
}

pub struct TabsStore {
    pub storage: Mutex<TabsStorage>,
}

impl TabsStore {
    pub fn new(db_path: impl AsRef<Path>) -> Self {
        Self {
            storage: Mutex::new(TabsStorage::new(db_path)),
        }
    }

    #[cfg(test)]
    pub fn new_with_mem_path(db_path: &str) -> Self {
        Self {
            storage: Mutex::new(TabsStorage::new_with_mem_path(db_path)),
        }
    }

    pub fn set_local_tabs(&self, local_state: Vec<RemoteTab>) {
        self.storage.lock().unwrap().update_local_state(local_state);
    }

    // like remote_tabs, but serves the uniffi layer
    pub fn get_all(&self) -> Vec<ClientRemoteTabs> {
        match self.remote_tabs() {
            Some(list) => list,
            None => vec![],
        }
    }

    pub fn remote_tabs(&self) -> Option<Vec<ClientRemoteTabs>> {
        self.storage.lock().unwrap().get_remote_tabs()
    }

    /// A convenience wrapper around sync_multiple.
    pub fn sync(
        self: Arc<Self>,
        storage_init: &Sync15StorageClientInit,
        root_sync_key: &KeyBundle,
        local_id: &str,
    ) -> Result<telemetry::SyncTelemetryPing> {
        let mut mem_cached_state = MemoryCachedState::default();
        let mut engine = TabsEngine::new(Arc::clone(&self));
        // Since we are syncing without the sync manager, there's no
        // command processor, therefore no clients engine, and in
        // consequence `TabsStore::prepare_for_sync` is never called
        // which means our `local_id` will never be set.
        // Do it here.
        engine.local_id = RefCell::new(local_id.to_owned());

        let mut result = sync_multiple(
            &[&engine],
            &mut None,
            &mut mem_cached_state,
            storage_init,
            root_sync_key,
            &NeverInterrupts,
            None,
        );

        // for b/w compat reasons, we do some dances with the result.
        // XXX - note that this means telemetry isn't going to be reported back
        // to the app - we need to check with lockwise about whether they really
        // need these failures to be reported or whether we can loosen this.
        if let Err(e) = result.result {
            return Err(e.into());
        }
        match result.engine_results.remove("tabs") {
            None | Some(Ok(())) => Ok(result.telemetry),
            Some(Err(e)) => Err(e.into()),
        }
    }

    // This allows the embedding app to say "make this instance available to
    // the sync manager". The implementation is more like "offer to sync mgr"
    // (thereby avoiding us needing to link with the sync manager) but
    // `register_with_sync_manager()` is logically what's happening so that's
    // the name it gets.
    pub fn register_with_sync_manager(self: Arc<Self>) {
        let mut state = STORE_FOR_MANAGER.lock().unwrap();
        *state = Arc::downgrade(&self);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_sync_manager_registration() {
        let store = Arc::new(TabsStore::new_with_mem_path("test"));
        assert_eq!(Arc::strong_count(&store), 1);
        assert_eq!(Arc::weak_count(&store), 0);
        Arc::clone(&store).register_with_sync_manager();
        assert_eq!(Arc::strong_count(&store), 1);
        assert_eq!(Arc::weak_count(&store), 1);
        let registered = STORE_FOR_MANAGER
            .lock()
            .unwrap()
            .upgrade()
            .expect("should upgrade");
        assert!(Arc::ptr_eq(&store, &registered));
        drop(registered);
        // should be no new references
        assert_eq!(Arc::strong_count(&store), 1);
        assert_eq!(Arc::weak_count(&store), 1);
        // dropping the registered object should drop the registration.
        drop(store);
        assert!(STORE_FOR_MANAGER.lock().unwrap().upgrade().is_none());
    }
}

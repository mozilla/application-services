/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::storage::{ClientRemoteTabs, RemoteTab};
use crate::sync::record::{TabsRecord, TabsRecordTab};
use crate::TabsStore;
use anyhow::Result;
use interrupt_support::NeverInterrupts;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use sync15::{
    clients::{self, RemoteClient},
    telemetry, CollectionRequest, DeviceType, EngineSyncAssociation, IncomingChangeset,
    OutgoingChangeset, Payload, ServerTimestamp, SyncEngine,
};
use sync15::{sync_multiple, MemoryCachedState, SyncEngineId};

use sync_guid::Guid;

const TTL_1_YEAR: u32 = 31_622_400;

// Our "sync manager" will use whatever is stashed here.
lazy_static::lazy_static! {
    // Mutex: just taken long enough to update the inner stuff
    pub(super) static ref STORE_FOR_MANAGER: Mutex<Weak<TabsStore>> = Mutex::new(Weak::new());
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

impl RemoteTab {
    fn from_record_tab(tab: &TabsRecordTab) -> Self {
        Self {
            title: tab.title.clone(),
            url_history: tab.url_history.clone(),
            icon: tab.icon.clone(),
            last_used: tab.last_used.checked_mul(1000).unwrap_or_default(),
        }
    }
    fn to_record_tab(&self) -> TabsRecordTab {
        TabsRecordTab {
            title: self.title.clone(),
            url_history: self.url_history.clone(),
            icon: self.icon.clone(),
            last_used: self.last_used.checked_div(1000).unwrap_or_default(),
        }
    }
}

impl ClientRemoteTabs {
    fn from_record_with_remote_client(
        client_id: String,
        remote_client: &RemoteClient,
        record: TabsRecord,
    ) -> Self {
        Self {
            client_id,
            client_name: remote_client.device_name.clone(),
            device_type: remote_client.device_type.unwrap_or(DeviceType::Unknown),
            remote_tabs: record.tabs.iter().map(RemoteTab::from_record_tab).collect(),
        }
    }

    fn from_record(client_id: String, record: TabsRecord) -> Self {
        Self {
            client_id,
            client_name: record.client_name,
            device_type: DeviceType::Unknown,
            remote_tabs: record.tabs.iter().map(RemoteTab::from_record_tab).collect(),
        }
    }
    fn to_record(&self) -> TabsRecord {
        TabsRecord {
            id: self.client_id.clone(),
            client_name: self.client_name.clone(),
            tabs: self
                .remote_tabs
                .iter()
                .map(RemoteTab::to_record_tab)
                .collect(),
            ttl: TTL_1_YEAR,
        }
    }
}

pub struct TabsEngine {
    pub store: Arc<TabsStore>,
    remote_clients: RefCell<HashMap<String, RemoteClient>>,
    last_sync: Cell<Option<ServerTimestamp>>, // We use a cell because `sync_finished` doesn't take a mutable reference to &self.
    sync_store_assoc: RefCell<EngineSyncAssociation>,
    pub(crate) local_id: RefCell<String>,
}

impl TabsEngine {
    pub fn new(store: Arc<TabsStore>) -> Self {
        Self {
            store,
            remote_clients: RefCell::default(),
            last_sync: Cell::default(),
            sync_store_assoc: RefCell::new(EngineSyncAssociation::Disconnected),
            local_id: RefCell::default(), // Will get replaced in `prepare_for_sync`.
        }
    }
}

impl SyncEngine for TabsEngine {
    fn collection_name(&self) -> std::borrow::Cow<'static, str> {
        "tabs".into()
    }

    fn prepare_for_sync(&self, get_client_data: &dyn Fn() -> clients::ClientData) -> Result<()> {
        let data = get_client_data();
        self.remote_clients.replace(data.recent_clients);
        self.local_id.replace(data.local_client_id);
        Ok(())
    }

    fn apply_incoming(
        &self,
        inbound: Vec<IncomingChangeset>,
        telem: &mut telemetry::Engine,
    ) -> Result<OutgoingChangeset> {
        assert_eq!(inbound.len(), 1, "only requested one set of records");
        let inbound = inbound.into_iter().next().unwrap();
        let mut incoming_telemetry = telemetry::EngineIncoming::new();
        let local_id = self.local_id.borrow().clone();
        let mut remote_tabs = Vec::with_capacity(inbound.changes.len());

        for incoming in inbound.changes {
            if incoming.0.id() == local_id {
                // That's our own record, ignore it.
                continue;
            }
            let record = match TabsRecord::from_payload(incoming.0) {
                Ok(record) => record,
                Err(e) => {
                    log::warn!("Error deserializing incoming record: {}", e);
                    incoming_telemetry.failed(1);
                    continue;
                }
            };
            let id = record.id.clone();
            let crt = if let Some(remote_client) = self.remote_clients.borrow().get(&id) {
                ClientRemoteTabs::from_record_with_remote_client(
                    remote_client
                        .fxa_device_id
                        .as_ref()
                        .unwrap_or(&id)
                        .to_owned(),
                    remote_client,
                    record,
                )
            } else {
                // A record with a device that's not in our remote clients seems unlikely, but
                // could happen - in most cases though, it will be due to a disconnected client -
                // so we really should consider just dropping it? (Sadly though, it does seem
                // possible it's actually a very recently connected client, so we keep it)
                log::info!(
                    "Storing tabs from a client that doesn't appear in the devices list: {}",
                    id,
                );
                ClientRemoteTabs::from_record(id, record)
            };
            remote_tabs.push(crt);
        }

        let mut outgoing = OutgoingChangeset::new("tabs", inbound.timestamp);
        // We want to keep the mutex for as short as possible
        let local_tabs = {
            let mut storage = self.store.storage.lock().unwrap();
            storage.replace_remote_tabs(remote_tabs)?;
            storage.prepare_local_tabs_for_upload()
        };
        if let Some(local_tabs) = local_tabs {
            let (client_name, device_type) = self
                .remote_clients
                .borrow()
                .get(&local_id)
                .map(|client| {
                    (
                        client.device_name.clone(),
                        client.device_type.unwrap_or(DeviceType::Unknown),
                    )
                })
                .unwrap_or_else(|| (String::new(), DeviceType::Unknown));
            let local_record = ClientRemoteTabs {
                client_id: local_id,
                client_name,
                device_type,
                remote_tabs: local_tabs.to_vec(),
            };
            let payload = Payload::from_record(local_record.to_record())?;
            log::trace!("outgoing {:?}", payload);
            outgoing.changes.push(payload);
        }
        telem.incoming(incoming_telemetry);
        Ok(outgoing)
    }

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: Vec<Guid>,
    ) -> Result<()> {
        log::info!(
            "sync completed after uploading {} records",
            records_synced.len()
        );
        self.last_sync.set(Some(new_timestamp));
        Ok(())
    }

    fn get_collection_requests(
        &self,
        server_timestamp: ServerTimestamp,
    ) -> Result<Vec<CollectionRequest>> {
        let since = self.last_sync.get().unwrap_or_default();
        Ok(if since == server_timestamp {
            vec![]
        } else {
            vec![CollectionRequest::new("tabs").full().newer_than(since)]
        })
    }

    fn get_sync_assoc(&self) -> Result<EngineSyncAssociation> {
        Ok(self.sync_store_assoc.borrow().clone())
    }

    fn reset(&self, assoc: &EngineSyncAssociation) -> Result<()> {
        self.remote_clients.borrow_mut().clear();
        self.sync_store_assoc.replace(assoc.clone());
        self.last_sync.set(None);
        self.store.storage.lock().unwrap().wipe_remote_tabs()?;
        Ok(())
    }

    fn wipe(&self) -> Result<()> {
        self.reset(&EngineSyncAssociation::Disconnected)?;
        // not clear why we need to wipe the local tabs - the app is just going
        // to re-add them?
        self.store.storage.lock().unwrap().wipe_local_tabs();
        Ok(())
    }
}

impl crate::TabsStore {
    pub fn reset(self: Arc<Self>) -> crate::error::Result<()> {
        let engine = TabsEngine::new(Arc::clone(&self));
        engine.reset(&EngineSyncAssociation::Disconnected)?;
        Ok(())
    }

    /// A convenience wrapper around sync_multiple.
    pub fn sync(
        self: Arc<Self>,
        key_id: String,
        access_token: String,
        sync_key: String,
        tokenserver_url: String,
        local_id: String,
    ) -> crate::error::Result<String> {
        let mut mem_cached_state = MemoryCachedState::default();
        let mut engine = TabsEngine::new(Arc::clone(&self));

        // Since we are syncing without the sync manager, there's no
        // command processor, therefore no clients engine, and in
        // consequence `TabsStore::prepare_for_sync` is never called
        // which means our `local_id` will never be set.
        // Do it here.
        engine.local_id = RefCell::new(local_id);

        let storage_init = &sync15::Sync15StorageClientInit {
            key_id,
            access_token,
            tokenserver_url: url::Url::parse(tokenserver_url.as_str())?,
        };
        let root_sync_key = &sync15::KeyBundle::from_ksync_base64(sync_key.as_str())?;

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
            None | Some(Ok(())) => Ok(serde_json::to_string(&result.telemetry)?),
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
pub mod test {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_incoming_tabs() {
        env_logger::try_init().ok();

        let engine = TabsEngine::new(Arc::new(TabsStore::new_with_mem_path("test")));

        let records = vec![
            json!({
                "id": "device-no-tabs",
                "clientName": "device with no tabs",
                "tabs": [],
            }),
            json!({
                "id": "device-with-a-tab",
                "clientName": "device with a tab",
                "tabs": [{
                    "title": "the title",
                    "urlHistory": [
                        "https://mozilla.org/"
                    ],
                    "icon": "https://mozilla.org/icon",
                    "lastUsed": 1643764207
                }]
            }),
            // This has the main payload as OK but the tabs part invalid.
            json!({
                "id": "device-with-invalid-tab",
                "clientName": "device with a tab",
                "tabs": [{
                    "foo": "bar",
                }]
            }),
            // We want this to be a valid payload but an invalid tab - so it needs an ID.
            json!({
                "id": "invalid-tab",
                "foo": "bar"
            }),
        ];

        let mut incoming = IncomingChangeset::new(engine.collection_name(), ServerTimestamp(0));
        for record in records {
            let payload = Payload::from_json(record).unwrap();
            incoming.changes.push((payload, ServerTimestamp(0)));
        }
        let outgoing = engine
            .apply_incoming(vec![incoming], &mut telemetry::Engine::new("tabs"))
            .expect("Should apply incoming and stage outgoing records");

        assert!(outgoing.changes.is_empty());

        // now check the store has what we think it has.
        let mut storage = engine.store.storage.lock().unwrap();
        let mut crts = storage.get_remote_tabs().expect("should work");
        crts.sort_by(|a, b| a.client_name.partial_cmp(&b.client_name).unwrap());
        assert_eq!(crts.len(), 2, "we currently include devices with no tabs");
        let crt = &crts[0];
        assert_eq!(crt.client_name, "device with a tab");
        assert_eq!(crt.device_type, DeviceType::Unknown);
        assert_eq!(crt.remote_tabs.len(), 1);
        assert_eq!(crt.remote_tabs[0].title, "the title");

        let crt = &crts[1];
        assert_eq!(crt.client_name, "device with no tabs");
        assert_eq!(crt.device_type, DeviceType::Unknown);
        assert_eq!(crt.remote_tabs.len(), 0);
    }

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

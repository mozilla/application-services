/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::storage::{ClientRemoteTabs, RemoteTab};
use crate::store::TabsStore;
use crate::sync::record::{TabsRecord, TabsRecordTab};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use sync15::engine::{
    CollectionRequest, EngineSyncAssociation, IncomingChangeset, OutgoingChangeset, SyncEngine,
    SyncEngineId,
};
use sync15::{telemetry, ClientData, DeviceType, Payload, RemoteClient, ServerTimestamp};
use sync_guid::Guid;

const TTL_1_YEAR: u32 = 31_622_400;

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

impl RemoteTab {
    fn from_record_tab(tab: &TabsRecordTab) -> Self {
        Self {
            title: tab.title.clone(),
            url_history: tab.url_history.clone(),
            icon: tab.icon.clone(),
            last_used: tab.last_used.checked_mul(1000).unwrap_or_default(),
        }
    }
    pub(super) fn to_record_tab(&self) -> TabsRecordTab {
        TabsRecordTab {
            title: self.title.clone(),
            url_history: self.url_history.clone(),
            icon: self.icon.clone(),
            last_used: self.last_used.checked_div(1000).unwrap_or_default(),
        }
    }
}

// This is the implementation of syncing, which is used by the 2 different "sync engines"
// (We hope to get these 2 engines even closer in the future, but for now, we suck this up)
pub struct TabsSyncImpl {
    pub(super) store: Arc<TabsStore>,
    remote_clients: HashMap<String, RemoteClient>,
    pub(super) last_sync: Option<ServerTimestamp>,
    sync_store_assoc: EngineSyncAssociation,
    pub(super) local_id: String,
}

impl TabsSyncImpl {
    pub fn new(store: Arc<TabsStore>) -> Self {
        Self {
            store,
            remote_clients: HashMap::new(),
            last_sync: None,
            sync_store_assoc: EngineSyncAssociation::Disconnected,
            local_id: Default::default(),
        }
    }

    pub fn prepare_for_sync(&mut self, client_data: ClientData) -> Result<()> {
        self.remote_clients = client_data.recent_clients;
        self.local_id = client_data.local_client_id;
        Ok(())
    }

    pub fn apply_incoming(&mut self, inbound: Vec<TabsRecord>) -> Result<Option<TabsRecord>> {
        let local_id = self.local_id.clone();
        let mut remote_tabs = Vec::with_capacity(inbound.len());

        for record in inbound {
            if record.id == local_id {
                // That's our own record, ignore it.
                continue;
            }
            let id = record.id.clone();
            let crt = if let Some(remote_client) = self.remote_clients.get(&id) {
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

        // We want to keep the mutex for as short as possible
        let local_tabs = {
            let mut storage = self.store.storage.lock().unwrap();
            storage.replace_remote_tabs(remote_tabs)?;
            storage.prepare_local_tabs_for_upload()
        };
        let outgoing = if let Some(local_tabs) = local_tabs {
            let (client_name, device_type) = self
                .remote_clients
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
            log::trace!("outgoing {:?}", local_record);
            Some(local_record.to_record())
        } else {
            None
        };
        Ok(outgoing)
    }

    pub fn sync_finished(
        &mut self,
        new_timestamp: ServerTimestamp,
        records_synced: &[Guid],
    ) -> Result<()> {
        log::info!(
            "sync completed after uploading {} records",
            records_synced.len()
        );
        self.last_sync = Some(new_timestamp);
        Ok(())
    }

    pub fn reset(&mut self, assoc: EngineSyncAssociation) -> Result<()> {
        self.remote_clients.clear();
        self.sync_store_assoc = assoc;
        self.last_sync = None;
        self.store.storage.lock().unwrap().wipe_remote_tabs()?;
        Ok(())
    }

    pub fn wipe(&mut self) -> Result<()> {
        self.reset(EngineSyncAssociation::Disconnected)?;
        // not clear why we need to wipe the local tabs - the app is just going
        // to re-add them?
        self.store.storage.lock().unwrap().wipe_local_tabs();
        Ok(())
    }

    pub fn get_sync_assoc(&self) -> &EngineSyncAssociation {
        &self.sync_store_assoc
    }
}

// This is the "SyncEngine" used when syncing via the Sync Manager.
pub struct TabsEngine {
    pub sync_impl: Mutex<TabsSyncImpl>,
}

impl TabsEngine {
    pub fn new(store: Arc<TabsStore>) -> Self {
        Self {
            sync_impl: Mutex::new(TabsSyncImpl::new(store)),
        }
    }
}

impl SyncEngine for TabsEngine {
    fn collection_name(&self) -> std::borrow::Cow<'static, str> {
        "tabs".into()
    }

    fn prepare_for_sync(&self, get_client_data: &dyn Fn() -> ClientData) -> Result<()> {
        self.sync_impl
            .lock()
            .unwrap()
            .prepare_for_sync(get_client_data())
    }

    fn apply_incoming(
        &self,
        inbound: Vec<IncomingChangeset>,
        telem: &mut telemetry::Engine,
    ) -> Result<OutgoingChangeset> {
        assert_eq!(inbound.len(), 1, "only requested one set of records");
        let inbound = inbound.into_iter().next().unwrap();
        let mut incoming_telemetry = telemetry::EngineIncoming::new();
        let mut incoming_records = Vec::with_capacity(inbound.changes.len());

        for incoming in inbound.changes {
            let record = match TabsRecord::from_payload(incoming.0) {
                Ok(record) => record,
                Err(e) => {
                    log::warn!("Error deserializing incoming record: {}", e);
                    incoming_telemetry.failed(1);
                    continue;
                }
            };
            incoming_records.push(record);
        }

        let outgoing_record = self
            .sync_impl
            .lock()
            .unwrap()
            .apply_incoming(incoming_records)?;

        let mut outgoing = OutgoingChangeset::new("tabs", inbound.timestamp);
        if let Some(outgoing_record) = outgoing_record {
            let payload = Payload::from_record(outgoing_record)?;
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
        self.sync_impl
            .lock()
            .unwrap()
            .sync_finished(new_timestamp, &records_synced)
    }

    fn get_collection_requests(
        &self,
        server_timestamp: ServerTimestamp,
    ) -> Result<Vec<CollectionRequest>> {
        let since = self.sync_impl.lock().unwrap().last_sync.unwrap_or_default();
        Ok(if since == server_timestamp {
            vec![]
        } else {
            vec![CollectionRequest::new("tabs").full().newer_than(since)]
        })
    }

    fn get_sync_assoc(&self) -> Result<EngineSyncAssociation> {
        Ok(self.sync_impl.lock().unwrap().get_sync_assoc().clone())
    }

    fn reset(&self, assoc: &EngineSyncAssociation) -> Result<()> {
        self.sync_impl.lock().unwrap().reset(assoc.clone())
    }

    fn wipe(&self) -> Result<()> {
        self.sync_impl.lock().unwrap().wipe()
    }
}

impl crate::TabsStore {
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
    use sync15::DeviceType;

    #[test]
    fn test_incoming_tabs() {
        env_logger::try_init().ok();

        let engine = TabsEngine::new(Arc::new(TabsStore::new_with_mem_path("test-incoming")));

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
        let sync_impl = engine.sync_impl.lock().unwrap();
        let mut storage = sync_impl.store.storage.lock().unwrap();
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
        let store = Arc::new(TabsStore::new_with_mem_path("test-registration"));
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

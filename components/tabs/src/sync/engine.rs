/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::storage::{ClientRemoteTabs, RemoteTab};
use crate::sync::record::{TabsRecord, TabsRecordTab};
use crate::sync::store::TabsStore;
use anyhow::Result;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;
use sync15::{
    clients::{self, RemoteClient},
    telemetry, CollectionRequest, DeviceType, EngineSyncAssociation, IncomingChangeset,
    OutgoingChangeset, Payload, ServerTimestamp, SyncEngine,
};
use sync_guid::Guid;

const TTL_1_YEAR: u32 = 31_622_400;

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
}

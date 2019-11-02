/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::storage::TabsStorage;
use crate::storage::{ClientRemoteTabs, RemoteTab};
use crate::sync::record::{TabsRecord, TabsRecordTab};
use std::cell::Cell;
use std::result;
use sync15::{
    telemetry, CollectionRequest, IncomingChangeset, OutgoingChangeset, Payload, ServerTimestamp,
    Store, StoreSyncAssociation,
};
use sync_guid::Guid;

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
    fn from_record(client_id: String, record: TabsRecord) -> Self {
        Self {
            client_id,
            remote_tabs: record.tabs.iter().map(RemoteTab::from_record_tab).collect(),
        }
    }
    fn to_record(&self) -> TabsRecord {
        TabsRecord {
            id: self.client_id.clone(),
            tabs: self
                .remote_tabs
                .iter()
                .map(RemoteTab::to_record_tab)
                .collect(),
        }
    }
}

pub struct TabsStore<'a> {
    storage: &'a TabsStorage,
    last_sync: Cell<Option<ServerTimestamp>>, // We use a cell because `sync_finished` doesn't take a mutable reference to &self.
}

impl<'a> TabsStore<'a> {
    pub fn new(storage: &'a TabsStorage) -> Self {
        Self {
            storage,
            last_sync: Cell::default(),
        }
    }
}

impl<'a> Store for TabsStore<'a> {
    fn collection_name(&self) -> &'static str {
        "tabs"
    }

    fn apply_incoming(
        &self,
        inbound: IncomingChangeset,
        telem: &mut telemetry::Engine,
    ) -> result::Result<OutgoingChangeset, failure::Error> {
        let mut incoming_telemetry = telemetry::EngineIncoming::new();
        let local_id = self.storage.get_local_id();
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
            // TODO: this is wrong anything that doesn't use the sync manager crate,
            // we need to get fxa_client_id from the clients collection instead.
            let id = record.id.clone();
            remote_tabs.push(ClientRemoteTabs::from_record(id, record));
        }
        self.storage.replace_remote_tabs(remote_tabs);
        let mut outgoing = OutgoingChangeset::new("tabs".into(), inbound.timestamp);
        if let Some(local_tabs) = self.storage.get_local_tabs() {
            let local_record = ClientRemoteTabs {
                client_id: local_id.to_owned(),
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
    ) -> result::Result<(), failure::Error> {
        log::info!(
            "sync completed after uploading {} records",
            records_synced.len()
        );
        self.last_sync.set(Some(new_timestamp));
        Ok(())
    }

    fn get_collection_request(&self) -> result::Result<CollectionRequest, failure::Error> {
        let since = self.last_sync.get().unwrap_or_default();
        Ok(CollectionRequest::new("tabs").full().newer_than(since))
    }

    fn get_sync_assoc(&self) -> result::Result<StoreSyncAssociation, failure::Error> {
        // This will cause the sync manager to call `reset`, which does nothing.
        Ok(StoreSyncAssociation::Disconnected)
    }

    fn reset(&self, _assoc: &StoreSyncAssociation) -> result::Result<(), failure::Error> {
        // Do nothing!
        Ok(())
    }

    fn wipe(&self) -> result::Result<(), failure::Error> {
        // Do nothing!
        Ok(())
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::cell::RefCell;
use sync15::clients::DeviceType;

#[derive(Clone, Debug)]
pub struct RemoteTab {
    pub title: String,
    pub url_history: Vec<String>,
    pub icon: Option<String>,
    pub last_used: u64, // In ms.
}

#[derive(Clone, Debug)]
pub struct ClientRemoteTabs {
    pub client_id: String, // Corresponds to the `clients` collection ID of the client.
    pub client_name: String,
    pub device_type: DeviceType,
    pub remote_tabs: Vec<RemoteTab>,
}

pub struct TabsStorage {
    local_id: String,
    local_tabs: Option<Vec<RemoteTab>>,
    remote_tabs: RefCell<Option<Vec<ClientRemoteTabs>>>,
}

impl TabsStorage {
    pub fn new(local_id: &str) -> Self {
        Self {
            local_id: local_id.to_owned(),
            local_tabs: None,
            remote_tabs: RefCell::default(),
        }
    }

    pub fn get_local_id(&self) -> &str {
        &self.local_id
    }

    pub fn update_local_state(&mut self, local_state: Vec<RemoteTab>) {
        self.local_tabs.replace(local_state);
    }

    pub fn get_local_tabs(&self) -> Option<&Vec<RemoteTab>> {
        self.local_tabs.as_ref()
    }

    pub fn get_remote_tabs(&self) -> Option<Vec<ClientRemoteTabs>> {
        self.remote_tabs.borrow().clone()
    }

    pub(crate) fn replace_remote_tabs(&self, new_remote_tabs: Vec<ClientRemoteTabs>) {
        let mut remote_tabs = self.remote_tabs.borrow_mut();
        remote_tabs.replace(new_remote_tabs);
    }

    pub fn wipe(&self) {
        unimplemented!("Implement me");
    }
}

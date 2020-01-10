/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::{HashMap, HashSet};

use crate::{
    bso_record::Payload,
    changeset::{CollectionUpdate, IncomingChangeset, OutgoingChangeset},
    client::Sync15StorageClient,
    coll_state::CollState,
    collection_keys::CollectionKeys,
    key_bundle::KeyBundle,
    request::{CollectionRequest, InfoConfiguration},
    state::GlobalState,
};
use interrupt::Interruptee;
use sync15_traits::client::ClientData;

use super::{
    record::{ClientRecord, CommandRecord},
    ser::shrink_to_fit,
    Command, CommandProcessor, CommandStatus, RemoteClient,
};
use crate::error::Result;

const COLLECTION_NAME: &str = "clients";

/// The driver for the clients engine. Internal; split out from the `Engine`
/// struct to make testing easier.
struct Driver<'a> {
    command_processor: &'a dyn CommandProcessor,
    interruptee: &'a dyn Interruptee,
    config: &'a InfoConfiguration,
    recent_clients: HashMap<String, RemoteClient>,
}

impl<'a> Driver<'a> {
    fn new(
        command_processor: &'a dyn CommandProcessor,
        interruptee: &'a dyn Interruptee,
        config: &'a InfoConfiguration,
    ) -> Driver<'a> {
        Driver {
            command_processor,
            interruptee,
            config,
            recent_clients: HashMap::new(),
        }
    }

    fn note_recent_client(&mut self, client: &ClientRecord) {
        self.recent_clients.insert(client.id.clone(), client.into());
    }

    fn sync(&mut self, inbound: IncomingChangeset) -> Result<OutgoingChangeset> {
        let mut outgoing = OutgoingChangeset::new(COLLECTION_NAME.into(), inbound.timestamp);
        outgoing.timestamp = inbound.timestamp;

        self.interruptee.err_if_interrupted()?;
        let outgoing_commands = self.command_processor.fetch_outgoing_commands()?;

        let mut has_own_client_record = false;

        for (payload, _) in inbound.changes {
            self.interruptee.err_if_interrupted()?;

            // Unpack the client record. We should never have tombstones in the
            // clients collection, so we don't check for `is_tombstone`.
            // https://github.com/mozilla/application-services/issues/1801
            // tracks deleting these from the server.
            let mut client: ClientRecord = payload.into_record()?;

            if client.id == self.command_processor.settings().fxa_device_id {
                // If we see our own client record, apply any incoming commands,
                // remove them from the list, and reupload the record. Any
                // commands that we don't understand also go back in the list.
                // https://github.com/mozilla/application-services/issues/1800
                // tracks if that's the right thing to do.
                has_own_client_record = true;
                let mut current_client_record = self.current_client_record();
                for c in client.commands {
                    let status = match c.as_command() {
                        Some(command) => self.command_processor.apply_incoming_command(command)?,
                        None => CommandStatus::Unsupported,
                    };
                    match status {
                        CommandStatus::Applied => {}
                        CommandStatus::Ignored => {
                            log::debug!("Ignored command {:?}", c);
                        }
                        CommandStatus::Unsupported => {
                            log::warn!("Don't know how to apply command {:?}", c);
                            current_client_record.commands.push(c);
                        }
                    }
                }

                // The clients collection has a hard limit on the payload size,
                // after which the server starts rejecting our records. Large
                // command lists can cause us to exceed this, so we truncate
                // the list.
                shrink_to_fit(
                    &mut current_client_record.commands,
                    self.memcache_max_record_payload_size(),
                )?;

                // Add the new client record to our map of recently synced
                // clients, so that downstream consumers like synced tabs can
                // access them.
                self.note_recent_client(&current_client_record);

                // We always upload our own client record on each sync, even if it
                // doesn't change, to keep it fresh.
                outgoing
                    .changes
                    .push(Payload::from_record(current_client_record)?);
            } else {
                // Add the other client to our map of recently synced clients.
                self.note_recent_client(&client);

                // Bail if we don't have any outgoing commands to write into
                // the other client's record.
                if outgoing_commands.is_empty() {
                    continue;
                }

                // Determine if we have new commands, that aren't already in the
                // client's command list.
                let old_len = client.commands.len();
                let current_commands: HashSet<Command> = client
                    .commands
                    .iter()
                    .filter_map(|c| c.as_command())
                    .collect();
                let mut new_outgoing_commands = outgoing_commands
                    .difference(&current_commands)
                    .cloned()
                    .collect::<Vec<_>>();
                // Sort, to ensure deterministic ordering for tests.
                new_outgoing_commands.sort();
                client
                    .commands
                    .extend(new_outgoing_commands.into_iter().map(CommandRecord::from));
                if client.commands.len() == old_len {
                    continue;
                }

                // Hooray, we added new commands! Make sure the record still
                // fits in the maximum record size, or the server will reject
                // our upload.
                shrink_to_fit(
                    &mut client.commands,
                    self.memcache_max_record_payload_size(),
                )?;

                outgoing.changes.push(Payload::from_record(client)?);
            }
        }

        // Upload a record for our own client, if we didn't replace it already.
        if !has_own_client_record {
            let current_client_record = self.current_client_record();
            self.note_recent_client(&current_client_record);
            outgoing
                .changes
                .push(Payload::from_record(current_client_record)?);
        }

        Ok(outgoing)
    }

    /// Builds a fresh client record for this device.
    fn current_client_record(&self) -> ClientRecord {
        let settings = self.command_processor.settings();
        ClientRecord {
            id: settings.fxa_device_id.clone(),
            name: settings.device_name.clone(),
            typ: Some(settings.device_type.as_str().into()),
            commands: Vec::new(),
            fxa_device_id: Some(settings.fxa_device_id.clone()),
            version: None,
            protocols: vec!["1.5".into()],
            form_factor: None,
            os: None,
            app_package: None,
            application: None,
            device: None,
        }
    }

    fn max_record_payload_size(&self) -> usize {
        let payload_max = self.config.max_record_payload_bytes;
        if payload_max <= self.config.max_post_bytes {
            self.config.max_post_bytes.saturating_sub(4096)
        } else {
            payload_max
        }
    }

    /// Collections stored in memcached ("tabs", "clients" or "meta") have a
    /// different max size than ones stored in the normal storage server db.
    /// In practice, the real limit here is 1M (bug 1300451 comment 40), but
    /// there's overhead involved that is hard to calculate on the client, so we
    /// use 512k to be safe (at the recommendation of the server team). Note
    /// that if the server reports a lower limit (via info/configuration), we
    /// respect that limit instead. See also bug 1403052.
    fn memcache_max_record_payload_size(&self) -> usize {
        self.max_record_payload_size().min(512 * 1024)
    }
}

pub struct Engine<'a> {
    pub command_processor: &'a dyn CommandProcessor,
    pub interruptee: &'a dyn Interruptee,
    pub recent_clients: HashMap<String, RemoteClient>,
}

impl<'a> Engine<'a> {
    /// Creates a new clients engine that delegates to the given command
    /// processor to apply incoming commands.
    pub fn new<'b>(
        command_processor: &'b dyn CommandProcessor,
        interruptee: &'b dyn Interruptee,
    ) -> Engine<'b> {
        Engine {
            command_processor,
            interruptee,
            recent_clients: HashMap::new(),
        }
    }

    /// Syncs the clients collection. This works a little differently than
    /// other collections:
    ///
    ///   1. It can't be disabled or declined.
    ///   2. The sync ID and last sync time aren't meaningful, since we always
    ///      fetch all client records on every sync. As such, the
    ///      `LocalCollStateMachine` that we use for other engines doesn't
    ///      apply to it.
    ///   3. It doesn't persist state directly, but relies on the sync manager
    ///      to persist device settings, and process commands.
    ///   4. Failing to sync the clients collection is fatal, and aborts the
    ///      sync.
    ///
    /// For these reasons, we implement this engine directly in the `sync15`
    /// crate, and provide a specialized `sync` method instead of implementing
    /// `sync15::Store`.
    pub fn sync(
        &mut self,
        storage_client: &Sync15StorageClient,
        global_state: &GlobalState,
        root_sync_key: &KeyBundle,
    ) -> Result<()> {
        log::info!("Syncing collection clients");

        let coll_keys =
            CollectionKeys::from_encrypted_bso(global_state.keys.clone(), &root_sync_key)?;
        let mut coll_state = CollState {
            config: global_state.config.clone(),
            last_modified: global_state
                .collections
                .get(COLLECTION_NAME)
                .cloned()
                .unwrap_or_default(),
            key: coll_keys.key_for_collection(COLLECTION_NAME).clone(),
        };

        let inbound = self.fetch_incoming(&storage_client, &mut coll_state)?;

        let mut driver = Driver::new(
            self.command_processor,
            self.interruptee,
            &global_state.config,
        );

        let outgoing = driver.sync(inbound)?;
        self.recent_clients = driver.recent_clients;

        coll_state.last_modified = outgoing.timestamp;

        self.interruptee.err_if_interrupted()?;
        let upload_info =
            CollectionUpdate::new_from_changeset(&storage_client, &coll_state, outgoing, true)?
                .upload()?;

        log::info!(
            "Upload success ({} records success, {} records failed)",
            upload_info.successful_ids.len(),
            upload_info.failed_ids.len()
        );

        log::info!("Finished syncing clients");
        Ok(())
    }

    fn fetch_incoming(
        &self,
        storage_client: &Sync15StorageClient,
        coll_state: &mut CollState,
    ) -> Result<IncomingChangeset> {
        // Note that, unlike other stores, we always fetch the full collection
        // on every sync, so `inbound` will return all clients, not just the
        // ones that changed since the last sync.
        let coll_request = CollectionRequest::new(COLLECTION_NAME).full();

        self.interruptee.err_if_interrupted()?;
        let inbound = crate::changeset::fetch_incoming(
            &storage_client,
            coll_state,
            COLLECTION_NAME.into(),
            &coll_request,
        )?;

        Ok(inbound)
    }

    pub fn local_client_id(&self) -> String {
        // Bit dirty but it's the easiest way to reach to our own
        // device ID without refactoring the whole sync manager crate.
        self.command_processor.settings().fxa_device_id.clone()
    }

    pub fn get_client_data(&self) -> ClientData {
        ClientData {
            local_client_id: self.local_client_id(),
            recent_clients: self.recent_clients.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::result;

    use failure;
    use interrupt::NeverInterrupts;
    use serde_json::{json, Value};

    use crate::clients::{CommandStatus, DeviceType, Settings};
    use crate::util::ServerTimestamp;

    use super::*;

    struct TestProcessor {
        settings: Settings,
        outgoing_commands: HashSet<Command>,
    }

    impl CommandProcessor for TestProcessor {
        fn settings(&self) -> &Settings {
            &self.settings
        }

        fn apply_incoming_command(
            &self,
            command: Command,
        ) -> result::Result<CommandStatus, failure::Error> {
            Ok(if let Command::Reset(name) = command {
                if name == "forms" {
                    CommandStatus::Unsupported
                } else {
                    CommandStatus::Applied
                }
            } else {
                CommandStatus::Ignored
            })
        }

        fn fetch_outgoing_commands(&self) -> result::Result<HashSet<Command>, failure::Error> {
            Ok(self.outgoing_commands.clone())
        }
    }

    #[test]
    fn test_clients_sync() {
        let processor = TestProcessor {
            settings: Settings {
                fxa_device_id: "deviceAAAAAA".into(),
                device_name: "Laptop".into(),
                device_type: DeviceType::Desktop,
            },
            outgoing_commands: [
                Command::Wipe("bookmarks".into()),
                Command::Reset("history".into()),
            ]
            .iter()
            .cloned()
            .collect(),
        };

        let config = InfoConfiguration::default();

        let mut driver = Driver::new(&processor, &NeverInterrupts, &config);

        let clients = json!([{
            "id": "deviceBBBBBB",
            "name": "iPhone",
            "type": "mobile",
            "commands": [{
                "command": "resetEngine",
                "args": ["history"],
            }],
            "fxaDeviceId": "iPhooooooone",
            "protocols": ["1.5"],
            "device": "iPhone",
        }, {
            "id": "deviceCCCCCC",
            "name": "Fenix",
            "type": "mobile",
            "commands": [],
            "fxaDeviceId": "deviceCCCCCC",
        }, {
            "id": "deviceAAAAAA",
            "name": "Laptop with a different name",
            "type": "desktop",
            "commands": [{
                "command": "wipeEngine",
                "args": ["logins"]
            }, {
                "command": "displayURI",
                "args": ["http://example.com", "Fennec", "Example page"],
                "flowID": "flooooooooow",
            }, {
                "command": "resetEngine",
                "args": ["forms"],
            }, {
                "command": "logout",
                "args": [],
            }],
            "fxaDeviceId": "deviceAAAAAA",
        }]);
        let inbound = if let Value::Array(clients) = clients {
            let changes = clients
                .into_iter()
                .map(|c| (Payload::from_json(c).unwrap(), ServerTimestamp(0)))
                .collect();
            IncomingChangeset {
                changes,
                timestamp: ServerTimestamp(0),
                collection: COLLECTION_NAME.into(),
            }
        } else {
            unreachable!("`clients` must be an array of client records")
        };

        let mut outgoing = driver.sync(inbound).expect("Should sync clients");
        outgoing.changes.sort_by(|a, b| a.id.cmp(&b.id));

        // Make sure the list of recently synced remote clients is correct.
        let expected_ids = &["deviceAAAAAA", "deviceBBBBBB", "deviceCCCCCC"];
        let mut actual_ids = driver.recent_clients.keys().collect::<Vec<&String>>();
        actual_ids.sort();
        assert_eq!(actual_ids, expected_ids);

        let expected_remote_clients = &[
            RemoteClient {
                fxa_device_id: Some("deviceAAAAAA".to_string()),
                device_name: "Laptop".into(),
                device_type: Some(DeviceType::Desktop),
            },
            RemoteClient {
                fxa_device_id: Some("iPhooooooone".to_string()),
                device_name: "iPhone".into(),
                device_type: Some(DeviceType::Mobile),
            },
            RemoteClient {
                fxa_device_id: Some("deviceCCCCCC".to_string()),
                device_name: "Fenix".into(),
                device_type: Some(DeviceType::Mobile),
            },
        ];
        let actual_remote_clients = expected_ids
            .iter()
            .filter_map(|&id| driver.recent_clients.get(id))
            .cloned()
            .collect::<Vec<RemoteClient>>();
        assert_eq!(actual_remote_clients, expected_remote_clients);

        let expected = json!([{
            "id": "deviceAAAAAA",
            "name": "Laptop",
            "type": "desktop",
            "commands": [{
                "command": "displayURI",
                "args": ["http://example.com", "Fennec", "Example page"],
                "flowID": "flooooooooow",
            }, {
                "command": "resetEngine",
                "args": ["forms"],
            }, {
                "command": "logout",
                "args": [],
            }],
            "fxaDeviceId": "deviceAAAAAA",
            "protocols": ["1.5"],
        }, {
            "id": "deviceBBBBBB",
            "name": "iPhone",
            "type": "mobile",
            "commands": [{
                "command": "resetEngine",
                "args": ["history"],
            }, {
                "command": "wipeEngine",
                "args": ["bookmarks"],
            }],
            "fxaDeviceId": "iPhooooooone",
            "protocols": ["1.5"],
            "device": "iPhone",
        }, {
            "id": "deviceCCCCCC",
            "name": "Fenix",
            "type": "mobile",
            "commands": [{
                "command": "wipeEngine",
                "args": ["bookmarks"],
            }, {
                "command": "resetEngine",
                "args": ["history"],
            }],
            "fxaDeviceId": "deviceCCCCCC",
        }]);
        if let Value::Array(expected) = expected {
            for (i, record) in expected.into_iter().enumerate() {
                assert_eq!(outgoing.changes[i], Payload::from_json(record).unwrap());
            }
        } else {
            unreachable!("`expected_clients` must be an array of client records")
        }
    }

    #[test]
    fn test_fresh_client_record() {
        let processor = TestProcessor {
            settings: Settings {
                fxa_device_id: "deviceAAAAAA".into(),
                device_name: "Laptop".into(),
                device_type: DeviceType::Desktop,
            },
            outgoing_commands: HashSet::new(),
        };

        let config = InfoConfiguration::default();

        let mut driver = Driver::new(&processor, &NeverInterrupts, &config);

        let clients = json!([{
            "id": "deviceBBBBBB",
            "name": "iPhone",
            "type": "mobile",
            "commands": [{
                "command": "resetEngine",
                "args": ["history"],
            }],
            "fxaDeviceId": "iPhooooooone",
            "protocols": ["1.5"],
            "device": "iPhone",
        }]);

        let inbound = if let Value::Array(clients) = clients {
            let changes = clients
                .into_iter()
                .map(|c| (Payload::from_json(c).unwrap(), ServerTimestamp(0)))
                .collect();
            IncomingChangeset {
                changes,
                timestamp: ServerTimestamp(0),
                collection: COLLECTION_NAME.into(),
            }
        } else {
            unreachable!("`clients` must be an array of client records")
        };

        let mut outgoing = driver.sync(inbound).expect("Should sync clients");
        outgoing.changes.sort_by(|a, b| a.id.cmp(&b.id));

        // Make sure the list of recently synced remote clients is correct.
        let expected_ids = &["deviceAAAAAA", "deviceBBBBBB"];
        let mut actual_ids = driver.recent_clients.keys().collect::<Vec<&String>>();
        actual_ids.sort();
        assert_eq!(actual_ids, expected_ids);

        let expected_remote_clients = &[
            RemoteClient {
                fxa_device_id: Some("deviceAAAAAA".to_string()),
                device_name: "Laptop".into(),
                device_type: Some(DeviceType::Desktop),
            },
            RemoteClient {
                fxa_device_id: Some("iPhooooooone".to_string()),
                device_name: "iPhone".into(),
                device_type: Some(DeviceType::Mobile),
            },
        ];
        let actual_remote_clients = expected_ids
            .iter()
            .filter_map(|&id| driver.recent_clients.get(id))
            .cloned()
            .collect::<Vec<RemoteClient>>();
        assert_eq!(actual_remote_clients, expected_remote_clients);

        let expected = json!([{
            "id": "deviceAAAAAA",
            "name": "Laptop",
            "type": "desktop",
            "fxaDeviceId": "deviceAAAAAA",
            "protocols": ["1.5"],
        }]);
        if let Value::Array(expected) = expected {
            for (i, record) in expected.into_iter().enumerate() {
                assert_eq!(outgoing.changes[i], Payload::from_json(record).unwrap());
            }
        } else {
            unreachable!("`expected_clients` must be an array of client records")
        }
    }
}

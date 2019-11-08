/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashSet;

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

use super::{
    record::{ClientRecord, CommandRecord},
    ser::shrink_to_fit,
    Command, CommandProcessor, CommandStatus,
};
use crate::error::Result;

const COLLECTION_NAME: &str = "clients";

/// The driver for the clients engine. Internal; split out from the `Engine`
/// struct to make testing easier.
struct Driver<'a> {
    command_processor: &'a dyn CommandProcessor,
    interruptee: &'a dyn Interruptee,
    config: &'a InfoConfiguration,
}

impl<'a> Driver<'a> {
    fn sync(&self, inbound: IncomingChangeset) -> Result<OutgoingChangeset> {
        let mut outgoing = OutgoingChangeset::new(COLLECTION_NAME.into(), inbound.timestamp);
        outgoing.timestamp = inbound.timestamp;

        self.interruptee.err_if_interrupted()?;
        let outgoing_commands = self.command_processor.fetch_outgoing_commands()?;

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

                // We always upload our own client record on each sync, even if it
                // doesn't change, to keep it fresh.
                outgoing
                    .changes
                    .push(Payload::from_record(current_client_record)?);
            } else {
                // For other clients, write our outgoing commands into their
                // records. If we don't have any, we don't need to reupload the
                // other client's record.
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
    pub storage_client: &'a Sync15StorageClient,
    pub global_state: &'a GlobalState,
    pub root_sync_key: &'a KeyBundle,
}

impl<'a> Engine<'a> {
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
    pub fn sync(&self) -> Result<()> {
        log::info!("Syncing collection clients");

        let coll_keys = CollectionKeys::from_encrypted_bso(
            self.global_state.keys.clone(),
            &self.root_sync_key,
        )?;
        let mut coll_state = CollState {
            config: self.global_state.config.clone(),
            last_modified: self
                .global_state
                .collections
                .get(COLLECTION_NAME)
                .cloned()
                .unwrap_or_default(),
            key: coll_keys.key_for_collection(COLLECTION_NAME).clone(),
        };

        let inbound = self.fetch_incoming(&mut coll_state)?;

        let driver = Driver {
            command_processor: self.command_processor,
            interruptee: self.interruptee,
            config: &self.global_state.config,
        };

        let outgoing = driver.sync(inbound)?;
        coll_state.last_modified = outgoing.timestamp;

        self.interruptee.err_if_interrupted()?;
        let upload_info = CollectionUpdate::new_from_changeset(
            &self.storage_client,
            &coll_state,
            outgoing,
            true,
        )?
        .upload()?;

        log::info!(
            "Upload success ({} records success, {} records failed)",
            upload_info.successful_ids.len(),
            upload_info.failed_ids.len()
        );

        log::info!("Finished syncing clients");
        Ok(())
    }

    fn fetch_incoming(&self, coll_state: &mut CollState) -> Result<IncomingChangeset> {
        let coll_request = CollectionRequest::new(COLLECTION_NAME).full();

        self.interruptee.err_if_interrupted()?;
        let inbound = IncomingChangeset::fetch(
            &self.storage_client,
            coll_state,
            COLLECTION_NAME.into(),
            &coll_request,
        )?;

        Ok(inbound)
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

    #[test]
    fn tests_clients_sync() {
        struct TestProcessor(Settings);

        impl CommandProcessor for TestProcessor {
            fn settings(&self) -> &Settings {
                &self.0
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
                let mut commands = HashSet::new();
                commands.insert(Command::Wipe("bookmarks".into()));
                commands.insert(Command::Reset("history".into()));
                Ok(commands)
            }
        }

        let processor = TestProcessor(Settings {
            fxa_device_id: "deviceAAAAAA".into(),
            device_name: "Laptop".into(),
            device_type: DeviceType::Desktop,
        });
        let config = InfoConfiguration::default();

        let driver = Driver {
            command_processor: &processor,
            interruptee: &NeverInterrupts,
            config: &config,
        };

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
        };
    }
}

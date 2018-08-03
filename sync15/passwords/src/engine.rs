// Copyright 2018 Mozilla
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may not use
// this file except in compliance with the License. You may obtain a copy of the
// License at http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software distributed
// under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

use sync15_adapter as sync;
use self::sync::{
    ServerTimestamp,
    OutgoingChangeset,
    Payload,
};

use mentat::{
    self,
    DateTime,
    FromMillis,
    Utc,
};

use logins::{
    credentials,
    passwords,
    Credential,
    CredentialId,
    SyncGuid,
    ServerPassword,
    ensure_vocabulary,
};

use std::collections::BTreeSet;

use errors::{
    Sync15PasswordsError,
    Result,
};

// TODO: These probably don't all need to be public!
pub struct PasswordEngine {
    pub last_server_timestamp: ServerTimestamp,
    pub current_tx_id: Option<mentat::Entid>,
    pub store: mentat::store::Store,
}

impl PasswordEngine {

    pub fn new(mut store: mentat::store::Store) -> Result<PasswordEngine> {
        let last_server_timestamp: ServerTimestamp = { // Scope borrow of `store`.
            let mut in_progress = store.begin_transaction()?;

            ensure_vocabulary(&mut in_progress)?;

            let timestamp = passwords::get_last_server_timestamp(&in_progress)?;

            in_progress.commit()?;

            ServerTimestamp(timestamp.unwrap_or_default())
        };

        Ok(PasswordEngine {
            current_tx_id: None,
            last_server_timestamp,
            store,
        })
    }

    pub fn touch_credential(&mut self, id: String) -> Result<()> {
        let mut in_progress = self.store.begin_transaction()?;
        credentials::touch_by_id(&mut in_progress, CredentialId(id), None)?;
        in_progress.commit()?;
        Ok(())
    }

    pub fn delete_credential(&mut self, id: String) -> Result<()> {
        let mut in_progress = self.store.begin_transaction()?;
        credentials::delete_by_id(&mut in_progress, CredentialId(id))?;
        in_progress.commit()?;
        Ok(())
    }

    pub fn update_credential(&mut self, id: &str, updater: impl FnMut(&mut Credential)) -> Result<bool> {
        let mut in_progress = self.store.begin_transaction()?;

        let mut credential = credentials::get_credential(&in_progress, CredentialId(id.into()))?;
        if credential.as_mut().map(updater).is_none() {
            return Ok(false);
        }

        credentials::add_credential(&mut in_progress, credential.unwrap())?;
        in_progress.commit()?;
        Ok(true)
    }

    pub fn sync(
        &mut self,
        client: &sync::Sync15StorageClient,
        state: &sync::GlobalState,
    ) -> Result<()> {
        let ts = self.last_server_timestamp;
        sync::synchronize(client, state, self, "passwords".into(), ts, true)?;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<()> {
        { // Scope borrow of self.
            let mut in_progress = self.store.begin_transaction()?;
            passwords::reset_client(&mut in_progress)?;
            in_progress.commit()?;
        }

        self.last_server_timestamp = 0.0.into();

        Ok(())
    }

    pub fn wipe(&mut self) -> Result<()> {
        self.last_server_timestamp = 0.0.into();

        // let mut in_progress = store.begin_transaction().map_err(|_| "failed to begin_transaction")?;
        // // reset_client(&mut in_progress).map_err(|_| "failed to reset_client")?;
        // in_progress.commit().map_err(|_| "failed to commit")?;

        // self.save()?;
        Ok(())
    }

    pub fn get_unsynced_changes(&mut self) -> Result<(Vec<Payload>, ServerTimestamp)> {
        let mut result = vec![];

        let in_progress_read = self.store.begin_read()?;

        let deleted = passwords::get_deleted_sync_password_uuids_to_upload(&in_progress_read)?;
        debug!("{} deleted records to upload: {:?}", deleted.len(), deleted);

        for r in deleted {
            result.push(Payload::new_tombstone(r.0))
        }

        let modified = passwords::get_modified_sync_passwords_to_upload(&in_progress_read)?;
        debug!("{} modified records to upload: {:?}", modified.len(), modified.iter().map(|r| &r.uuid.0).collect::<Vec<_>>());

        for r in modified {
            result.push(Payload::from_record(r)?);
        }

        Ok((result, self.last_server_timestamp))
    }
}

impl sync::Store for PasswordEngine {
    type Error = Sync15PasswordsError;

    fn apply_incoming(
        &mut self,
        inbound: sync::IncomingChangeset
    ) -> Result<OutgoingChangeset> {
        debug!("Remote collection has {} changes timestamped at {}",
               inbound.changes.len(), inbound.timestamp);

        { // Scope borrow of inbound.changes.
            let (to_delete, to_apply): (Vec<_>, Vec<_>) = inbound.changes.iter().partition(|(payload, _)| payload.is_tombstone());
            debug!("{} records to delete: {:?}", to_delete.len(), to_delete);
            debug!("{} records to apply: {:?}", to_apply.len(), to_apply);
        }

        self.current_tx_id = { // Scope borrow of self.
            let mut in_progress = self.store.begin_transaction()?;

            for (payload, server_timestamp) in inbound.changes {
                if payload.is_tombstone() {
                    passwords::delete_by_sync_uuid(&mut in_progress, payload.id().into())?;
                } else {
                    debug!("Applying: {:?}", payload);

                    let mut server_password: ServerPassword = payload.clone().into_record()?;
                    server_password.modified = DateTime::<Utc>::from_millis(server_timestamp.as_millis() as i64);

                    passwords::apply_password(&mut in_progress, server_password)?;
                }
            }

            let current_tx_id = in_progress.last_tx_id();
            in_progress.commit()?;

            Some(current_tx_id)
        };

        let (outbound_changes, last_server_timestamp) = self.get_unsynced_changes()?;

        let outbound = OutgoingChangeset {
            changes: outbound_changes,
            timestamp: last_server_timestamp,
            collection: "passwords".into()
        };

        debug!("After applying incoming changes, local collection has {} outgoing changes timestamped at {}",
               outbound.changes.len(), outbound.timestamp);

        Ok(outbound)
    }

    fn sync_finished(&mut self, new_last_server_timestamp: ServerTimestamp, records_synced: &[String]) -> Result<()> {
        debug!("Synced {} outbound changes at remote timestamp {}", records_synced.len(), new_last_server_timestamp);
        for id in records_synced {
            trace!("  {:?}", id);
        }

        let current_tx_id = self.current_tx_id.unwrap(); // XXX

        { // Scope borrow of self.
            let mut in_progress = self.store.begin_transaction()?;

            let deleted = passwords::get_deleted_sync_password_uuids_to_upload(&in_progress)?;
            let deleted: BTreeSet<String> = deleted.into_iter().map(|x| x.0).collect();

            let (deleted, uploaded): (Vec<_>, Vec<_>) =
                records_synced.iter().cloned().partition(|id| deleted.contains(id));

            passwords::mark_synced_by_sync_uuids(&mut in_progress, uploaded.into_iter().map(SyncGuid).collect(), current_tx_id)?;
            passwords::delete_by_sync_uuids(&mut in_progress, deleted.into_iter().map(SyncGuid).collect())?;

            passwords::set_last_server_timestamp(&mut in_progress, new_last_server_timestamp.0)?;

            in_progress.commit()?;
        };

        self.last_server_timestamp = new_last_server_timestamp;
        Ok(())
    }
}

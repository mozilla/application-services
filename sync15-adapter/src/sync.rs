/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use changeset::{CollectionUpdate, IncomingChangeset, OutgoingChangeset};
use client::Sync15StorageClient;
use error::Error;
use failure;
use state::GlobalState;
use util::ServerTimestamp;

/// Low-level store functionality. Stores that need custom reconciliation logic should use this.
///
/// Different stores will produce errors of different types.  To accommodate this, we force them
/// all to return failure::Error, which we expose as ErrorKind::StoreError.
pub trait Store {
    fn apply_incoming(
        &self,
        inbound: IncomingChangeset
    ) -> Result<OutgoingChangeset, failure::Error>;

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: &[String],
    ) -> Result<(), failure::Error>;

    fn get_last_sync(&self) -> Result<Option<ServerTimestamp>, failure::Error>;

    fn set_last_sync(&self, last_sync: ServerTimestamp) -> Result<(), failure::Error>;

    fn reset(&self) -> Result<(), failure::Error>;

    fn wipe(&self) -> Result<(), failure::Error>;
}

pub fn synchronize(client: &Sync15StorageClient,
                   state: &GlobalState,
                   store: &Store,
                   collection: String,
                   timestamp: ServerTimestamp,
                   fully_atomic: bool) -> Result<(), Error>
{

    info!("Syncing collection {}", collection);
    let incoming_changes = IncomingChangeset::fetch(client, state, collection.clone(), timestamp)?;
    let last_changed_remote = incoming_changes.timestamp;

    info!("Downloaded {} remote changes", incoming_changes.changes.len());
    let mut outgoing = store.apply_incoming(incoming_changes)?;

    outgoing.timestamp = last_changed_remote;

    info!("Uploading {} outgoing changes", outgoing.changes.len());
    let upload_info =
        CollectionUpdate::new_from_changeset(client, state, outgoing, fully_atomic)?.upload()?;

    info!("Upload success ({} records success, {} records failed)",
          upload_info.successful_ids.len(),
          upload_info.failed_ids.len());

    store.sync_finished(upload_info.modified_timestamp, &upload_info.successful_ids)?;

    info!("Sync finished!");
    Ok(())
}

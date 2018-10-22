/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use changeset::{CollectionUpdate, IncomingChangeset, OutgoingChangeset};
use request::{CollectionRequest};
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
    fn collection_name(&self) -> String;

    fn apply_incoming(
        &self,
        inbound: IncomingChangeset
    ) -> Result<OutgoingChangeset, failure::Error>;

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: &[String],
    ) -> Result<(), failure::Error>;

    // The store is responsible for building the collection request. Engines
    // typically will store a lastModified timestamp and use that to build
    // a request saying "give me full records since that date" - however, other
    // engines might do something fancier. This could even later be extended
    // to handle "backfills" etc
    fn get_collection_request(&self) -> Result<CollectionRequest, failure::Error>;

    fn reset(&self) -> Result<(), failure::Error>;

    fn wipe(&self) -> Result<(), failure::Error>;
}

pub fn synchronize(client: &Sync15StorageClient,
                   state: &GlobalState,
                   store: &Store,
                   fully_atomic: bool) -> Result<(), Error>
{

    let collection = store.collection_name();
    info!("Syncing collection {}", collection);
    let collection_request = store.get_collection_request()?;
    let incoming_changes = IncomingChangeset::fetch(client, state, collection.clone(), &collection_request)?;
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

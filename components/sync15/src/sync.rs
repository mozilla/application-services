/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::changeset::{CollectionUpdate, IncomingChangeset, OutgoingChangeset};
use crate::client::Sync15StorageClient;
use crate::error::Error;
use crate::request::CollectionRequest;
use crate::state::GlobalState;
use crate::util::ServerTimestamp;
use crate::telemetry;

/// Low-level store functionality. Stores that need custom reconciliation logic should use this.
///
/// Different stores will produce errors of different types.  To accommodate this, we force them
/// all to return failure::Error, which we expose as ErrorKind::StoreError.
pub trait Store {
    fn collection_name(&self) -> &'static str;

    fn apply_incoming(
        &self,
        inbound: IncomingChangeset,
        incoming_telem: &mut telemetry::EngineIncoming,
    ) -> Result<(OutgoingChangeset), failure::Error>;

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: &[String],
    ) -> Result<(), failure::Error>;

    /// The store is responsible for building the collection request. Engines
    /// typically will store a lastModified timestamp and use that to build
    /// a request saying "give me full records since that date" - however, other
    /// engines might do something fancier. This could even later be extended
    /// to handle "backfills" etc
    fn get_collection_request(&self) -> Result<CollectionRequest, failure::Error>;

    fn reset(&self) -> Result<(), failure::Error>;

    fn wipe(&self) -> Result<(), failure::Error>;
}

pub fn synchronize(
    client: &Sync15StorageClient,
    state: &GlobalState,
    store: &Store,
    fully_atomic: bool,
) -> Result<telemetry::Engine, Error> {
    let collection = store.collection_name();
    let mut telem = telemetry::Engine::new(collection);
    log::info!("Syncing collection {}", collection);
    let collection_request = store.get_collection_request()?;
    let incoming_changes =
        IncomingChangeset::fetch(client, state, collection.into(), &collection_request)?;
    let last_changed_remote = incoming_changes.timestamp;

    log::info!(
        "Downloaded {} remote changes",
        incoming_changes.changes.len()
    );
    let mut incoming_telem = telemetry::EngineIncoming::new();
    let mut outgoing= store.apply_incoming(incoming_changes, &mut incoming_telem)?;
    telem = telem.incoming(incoming_telem);

    outgoing.timestamp = last_changed_remote;

    log::info!("Uploading {} outgoing changes", outgoing.changes.len());
    let upload_info =
        CollectionUpdate::new_from_changeset(client, state, outgoing, fully_atomic)?.upload()?;

    log::info!(
        "Upload success ({} records success, {} records failed)",
        upload_info.successful_ids.len(),
        upload_info.failed_ids.len()
    );
    // ideally we'd report this per-batch, but for now, let's just report it
    // as a total.
    telem = telem.outgoing(telemetry::EngineOutgoing::new().sent(upload_info.successful_ids.len() + upload_info.failed_ids.len()).failed(upload_info.failed_ids.len()));

    store.sync_finished(upload_info.modified_timestamp, &upload_info.successful_ids)?;

    log::info!("Sync finished!");
    Ok(telem)
}

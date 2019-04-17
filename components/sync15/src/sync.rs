/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::changeset::{CollectionUpdate, IncomingChangeset, OutgoingChangeset};
use crate::client::Sync15StorageClient;
use crate::coll_state::{LocalCollStateMachine, StoreSyncAssociation};
use crate::error::Error;
use crate::request::CollectionRequest;
use crate::state::GlobalState;
use crate::telemetry;
use crate::util::ServerTimestamp;
use interrupt::Interruptee;

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
    ) -> Result<OutgoingChangeset, failure::Error>;

    fn sync_finished(
        &self,
        new_timestamp: ServerTimestamp,
        records_synced: Vec<String>,
    ) -> Result<(), failure::Error>;

    /// The store is responsible for building the collection request. Engines
    /// typically will store a lastModified timestamp and use that to build
    /// a request saying "give me full records since that date" - however, other
    /// engines might do something fancier. This could even later be extended
    /// to handle "backfills" etc
    fn get_collection_request(&self) -> Result<CollectionRequest, failure::Error>;

    /// Get persisted sync IDs. If they don't match the global state we'll be
    /// `reset()` with the new IDs.
    fn get_sync_assoc(&self) -> Result<StoreSyncAssociation, failure::Error>;

    /// Reset the store without wiping local data, ready for a "first sync".
    /// `assoc` defines how this store is to be associated with sync.
    fn reset(&self, assoc: &StoreSyncAssociation) -> Result<(), failure::Error>;

    fn wipe(&self) -> Result<(), failure::Error>;
}

pub fn synchronize(
    client: &Sync15StorageClient,
    global_state: &GlobalState,
    store: &dyn Store,
    fully_atomic: bool,
    telem_engine: &mut telemetry::Engine,
    interruptee: &impl Interruptee,
) -> Result<(), Error> {
    let collection = store.collection_name();
    log::info!("Syncing collection {}", collection);

    // our global state machine is ready - get the collection machine going.
    let mut coll_state = match LocalCollStateMachine::get_state(store, global_state)? {
        Some(coll_state) => coll_state,
        None => {
            // XXX - this is either "error" or "declined".
            log::warn!(
                "can't setup for the {} collection - hopefully it works later",
                collection
            );
            return Ok(());
        }
    };

    let collection_request = store.get_collection_request()?;
    interruptee.err_if_interrupted()?;
    let incoming_changes = IncomingChangeset::fetch(
        client,
        &mut coll_state,
        collection.into(),
        &collection_request,
    )?;
    assert_eq!(incoming_changes.timestamp, coll_state.last_modified);

    log::info!(
        "Downloaded {} remote changes",
        incoming_changes.changes.len()
    );
    let new_timestamp = incoming_changes.timestamp;
    let mut telem_incoming = telemetry::EngineIncoming::new();
    let mut outgoing = store.apply_incoming(incoming_changes, &mut telem_incoming)?;
    telem_engine.incoming(telem_incoming);

    interruptee.err_if_interrupted()?;
    // xxx - duplication below smells wrong
    outgoing.timestamp = new_timestamp;
    coll_state.last_modified = new_timestamp;

    log::info!("Uploading {} outgoing changes", outgoing.changes.len());
    let upload_info =
        CollectionUpdate::new_from_changeset(client, &coll_state, outgoing, fully_atomic)?
            .upload()?;

    log::info!(
        "Upload success ({} records success, {} records failed)",
        upload_info.successful_ids.len(),
        upload_info.failed_ids.len()
    );
    // ideally we'd report this per-batch, but for now, let's just report it
    // as a total.
    let mut telem_outgoing = telemetry::EngineOutgoing::new();
    telem_outgoing.sent(upload_info.successful_ids.len() + upload_info.failed_ids.len());
    telem_outgoing.failed(upload_info.failed_ids.len());
    telem_engine.outgoing(telem_outgoing);

    store.sync_finished(upload_info.modified_timestamp, upload_info.successful_ids)?;

    log::info!("Sync finished!");
    Ok(())
}

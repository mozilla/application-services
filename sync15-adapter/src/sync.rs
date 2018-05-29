/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use changeset::{OutgoingChangeset, IncomingChangeset, CollectionUpdate};
use util::ServerTimestamp;
use record_id::Id;
use error::Result;
use service::Sync15Service;

/// Low-level store functionality. Stores that need custom reconciliation logic
/// should use this.
pub trait Store {
    fn apply_incoming(
        &mut self,
        inbound: IncomingChangeset
    ) -> Result<OutgoingChangeset>;

    fn sync_finished(
        &mut self,
        new_timestamp: ServerTimestamp,
        records_synced: &[Id],
    ) -> Result<()>;
}

pub fn synchronize(svc: &Sync15Service,
                   store: &mut Store,
                   collection: String,
                   timestamp: ServerTimestamp,
                   fully_atomic: bool) -> Result<()> {

    info!("Syncing collection {}", collection);
    let incoming_changes = IncomingChangeset::fetch(svc, collection.clone(), timestamp)?;
    let last_changed_remote = incoming_changes.timestamp;

    info!("Downloaded {} remote changes", incoming_changes.changes.len());
    let mut outgoing = store.apply_incoming(incoming_changes)?;

    assert_eq!(outgoing.timestamp, timestamp,
        "last sync timestamp should never change unless we change it");

    outgoing.timestamp = last_changed_remote;

    info!("Uploading {} outgoing changes", outgoing.changes.len());
    let upload_info = CollectionUpdate::new_from_changeset(svc, outgoing, fully_atomic)?.upload()?;

    info!("Upload success ({} records success, {} records failed)",
          upload_info.successful_ids.len(),
          upload_info.failed_ids.len());

    store.sync_finished(upload_info.modified_timestamp, &upload_info.successful_ids)?;

    info!("Sync finished!");
    Ok(())
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use changeset::{OutgoingChangeset, IncomingChangeset, CollectionUpdate};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use util::ServerTimestamp;
use bso_record::Payload;
use error::Result;
use service::Sync15Service;


// TODO: figure out how error reporting from the store will work (our
// Result is unlikely viable?)

#[derive(Debug, PartialEq, Clone)]
pub enum RecordChoice {
    TakeLocal,
    TakeRemote,
    TakeCombined(Payload),
}

/// Low-level store functionality. Stores that need custom reconciliation logic
/// should use this.
pub trait FullStore {
    fn apply_incoming(
        &mut self,
        inbound: IncomingChangeset
    ) -> Result<OutgoingChangeset>;

    fn sync_finished(
        &mut self,
        new_timestamp: ServerTimestamp,
        records_synced: &[String],
    ) -> Result<()>;
}

/// Higher-level store interface
pub trait BasicStore {

    /// Fetch all unsynced changes.
    fn get_unsynced_changes(&self) -> Result<OutgoingChangeset>;

    /// Apply the changes in the list, and update the sync timestamp.
    fn apply_reconciled_changes(&mut self,
                                record_changes: &[Payload],
                                new_last_sync: ServerTimestamp) -> Result<()>;

    /// Called when a sync finishes successfully. The store should remove all items in
    /// `synced_ids` from the set of items that need to be synced. and update
    fn sync_finished(&mut self,
                     new_last_sync: ServerTimestamp,
                     synced_ids: &[String]) -> Result<()>;

    fn reconcile_single(
        &mut self,
        remote: (&Payload, Duration),
        local: (&Payload, Duration)
    ) -> Result<RecordChoice> {
        Ok(match (local.0.is_tombstone(), remote.0.is_tombstone()) {
            (true, true) => {
                trace!("Both records are tombstones, doesn't matter which we take");
                RecordChoice::TakeRemote
            },
            (false, true) => {
                trace!("Modified locally, remote tombstone (keeping local)");
                RecordChoice::TakeLocal
            },
            (true, false) => {
                trace!("Modified on remote, locally tombstone (keeping remote)");
                RecordChoice::TakeRemote
            },
            (false, false) => {
                trace!("Modified on both remote and local, chosing on age (remote = {}s, local = {}s)",
                       remote.1.as_secs(), local.1.as_secs());

                // Take younger.
                if local.1 <= remote.1 {
                    RecordChoice::TakeLocal
                } else {
                    RecordChoice::TakeRemote
                }
            }
        })
    }
}

pub fn synchronize(svc: &Sync15Service,
                   store: &mut FullStore,
                   collection: String,
                   timestamp: ServerTimestamp,
                   fully_atomic: bool) -> Result<()> {

    info!("Syncing collection {}", collection);
    let incoming_changes = IncomingChangeset::fetch(svc, collection.clone(), timestamp)?;

    info!("Downloaded {} remote changes", incoming_changes.changes.len());
    let outgoing = store.apply_incoming(incoming_changes)?;

    info!("Uploading {} outgoing changes", outgoing.changes.len());
    let upload_info = CollectionUpdate::new_from_changeset(svc, outgoing, fully_atomic)?.upload()?;

    info!("Upload success ({} records success, {} records failed)",
          upload_info.successful_ids.len(),
          upload_info.failed_ids.len());

    store.sync_finished(upload_info.modified_timestamp, &upload_info.successful_ids)?;

    info!("Sync finished!");
    Ok(())
}

fn reconcile_and_apply(store: &mut BasicStore, inbound: IncomingChangeset) -> Result<OutgoingChangeset> {
    info!("Remote collection has {} changes", inbound.changes.len());

    let outbound = store.get_unsynced_changes()?;
    info!("Local collection has {} changes", outbound.changes.len());

    let reconciled = Reconciliation::between(store,
                                             outbound.changes,
                                             inbound.changes,
                                             inbound.timestamp)?;

    info!("Finished Reconciling: apply local {}, apply remote {}",
          reconciled.apply_as_incoming.len(),
          reconciled.apply_as_outgoing.len());

    store.apply_reconciled_changes(&reconciled.apply_as_incoming[..], inbound.timestamp)?;

    Ok(OutgoingChangeset {
        changes: reconciled.apply_as_outgoing.into_iter().map(|ct| (ct, UNIX_EPOCH)).collect(),
        timestamp: outbound.timestamp,
        collection: outbound.collection
    })
}

impl<T> FullStore for T where T: BasicStore {
    fn apply_incoming(
        &mut self,
        inbound: IncomingChangeset
    ) -> Result<OutgoingChangeset> {
        reconcile_and_apply(self, inbound)
    }

    fn sync_finished(&mut self, new_timestamp: ServerTimestamp, records_synced: &[String]) -> Result<()> {
        <Self as BasicStore>::sync_finished(self, new_timestamp, records_synced)
    }
}

#[derive(Clone, Debug)]
struct Reconciliation {
    apply_as_incoming: Vec<Payload>,
    apply_as_outgoing: Vec<Payload>,
}

impl Reconciliation {

    pub fn between(
        store: &mut BasicStore,
        local_changes: Vec<(Payload, SystemTime)>,
        remote_changes: Vec<(Payload, ServerTimestamp)>,
        remote_timestamp: ServerTimestamp
    ) -> Result<Reconciliation> {

        let mut result = Reconciliation {
            apply_as_incoming: vec![],
            apply_as_outgoing: vec![],
        };

        let mut local_lookup: HashMap<String, (Payload, Duration)> =
            local_changes.into_iter().map(|(record, time)| {
                (record.id.clone(),
                 (record,
                  time.elapsed().unwrap_or(Duration::new(0, 0))))
            }).collect();

        for (remote, remote_modified) in remote_changes.into_iter() {
            let remote_age = remote_modified.duration_since(remote_timestamp)
                                            .unwrap_or(Duration::new(0, 0));

            let (choice, local) =
                if let Some((local, local_age)) = local_lookup.remove(remote.id()) {
                    (store.reconcile_single((&remote, remote_age), (&local, local_age))?, Some(local))
                } else {
                    // No local change with that ID
                    (RecordChoice::TakeRemote, None)
                };

            match choice {
                RecordChoice::TakeRemote => result.apply_as_incoming.push(remote),
                RecordChoice::TakeLocal => result.apply_as_outgoing.push(local.unwrap()),
                RecordChoice::TakeCombined(ct) => {
                    result.apply_as_incoming.push(ct.clone());
                    result.apply_as_outgoing.push(ct);
                }
            }
        }

        for (_, (local_record, _)) in local_lookup.into_iter() {
            result.apply_as_outgoing.push(local_record);
        }

        Ok(result)
    }
}

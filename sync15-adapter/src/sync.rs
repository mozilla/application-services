/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use changeset::{OutgoingChangeset, IncomingChangeset, CollectionUpdate};
use std::time::Duration;
use std::collections::HashMap;
use util::ServerTimestamp;
use bso_record::{CleartextBso, Cleartext};
use error::Result;
use service::Sync15Service;
use request::UploadInfo;

// TODO: figure out how error reporting from the store will work (our
// Result is unlikely viable?)

/// Trait that should be implemented by clients.
pub trait Store {
    /// Fetch all unsynced changes.
    fn get_unsynced_changes(&self) -> Result<OutgoingChangeset>;

    /// Apply the changes in the list, and update the sync timestamp.
    ///
    /// Client is allowed to return a set of records to be uploaded (weak reupload),
    /// however, this should be done carefully to avoid sync fights.
    fn apply_changes(
        &mut self,
        record_changes: &[Cleartext],
        new_last_sync: ServerTimestamp
    ) -> Result<Vec<Cleartext>>;

    /// Called when a sync finishes successfully. The store should remove all items in
    /// `synced_ids` from the set of items that need to be synced. and update
    fn sync_finished(
        &mut self,
        synced_ids: &[&str],
        new_last_sync: ServerTimestamp
    ) -> Result<()>;
}

// The set of outgoing records is
//
// - The set of records initially marked as having changed.
// - Minus those that we reconciled in favor of the remote.
// - Plus the set of records that need weak upload.
fn build_outgoing(
    initial_changes: &OutgoingChangeset,
    reconciled: &Reconciliation,
    to_weak_upload: Vec<Cleartext>
) -> impl Iterator<Item = CleartextBso> {

    let mut outgoing: HashMap<String, Cleartext> =
        initial_changes.changes.iter()
                       .map(|(record, _)| (record.id().into(), record.clone()))
                       .collect::<HashMap<_, _>>();

    for r in reconciled.apply_as_outgoing.iter() {
        outgoing.remove(r.id());
    }

    for id in reconciled.skipped.iter() {
        outgoing.remove(id);
    }

    for r in to_weak_upload {
        let id = r.id().into();
        outgoing.insert(id, r);
    }

    let collection = initial_changes.collection.clone();

    outgoing.into_iter().map(move |(_, record)|
        record.into_bso(collection.clone()))
}

pub fn sync(svc: &Sync15Service, store: &mut Store, fully_atomic: bool) -> Result<UploadInfo> {
    let changed = store.get_unsynced_changes()?;
    let incoming_changes = IncomingChangeset::fetch(svc, changed.collection.clone(), changed.timestamp)?;
    let reconciled = Reconciliation::between(&changed, &incoming_changes);

    let to_weak_upload = store.apply_changes(&reconciled.apply_as_incoming,
                                             incoming_changes.timestamp)?;

    let key_bundle = svc.key_for_collection(&changed.collection)?;

    let outgoing = build_outgoing(&changed, &reconciled, to_weak_upload)
        .map(|record| record.encrypt(key_bundle))
        .collect::<Result<Vec<_>>>()?;

    let updater = CollectionUpdate::new(svc,
                                        changed.collection.clone(),
                                        incoming_changes.timestamp,
                                        outgoing,
                                        fully_atomic);

    let upload_info = updater.upload()?;

    let changed_ids = changed.changes.iter().map(|r| r.0.id()).collect::<Vec<_>>();
    store.sync_finished(&changed_ids, upload_info.modified_timestamp)?;

    Ok(upload_info)
}

#[derive(Debug, PartialEq)]
enum Choice {
    Skip,
    Local(Cleartext),
    Remote(Cleartext)
}

#[derive(Clone, Debug)]
struct Reconciliation {
    apply_as_incoming: Vec<Cleartext>,
    apply_as_outgoing: Vec<Cleartext>,
    skipped: Vec<String>,
}

impl Reconciliation {

    fn reconcile_one(
        remote: &Cleartext,
        remote_age: Duration,
        local: Option<&(&Cleartext, Duration)>
    ) -> Choice {

        let (local, local_age) = match local {
            Some(&local) => local,
            None => {
                if !remote.is_tombstone() {
                    return Choice::Remote(remote.clone());
                }
                return Choice::Skip;
            }
        };

        match (local.is_tombstone(), remote.is_tombstone()) {
            // Both tombstones (nothing to do)
            (true, true) => return Choice::Skip,
            // Modified locally, remote tombstone. Take local (undelete)
            (false, true) => return Choice::Local(local.clone()),
            // Deleted locally, remote update. Take remote (undelete)
            (true, false) => return Choice::Remote(remote.clone()),
            // Moth modified locally.
            (false, false) => {
                // Take younger.
                return if local_age <= remote_age {
                    Choice::Local(local.clone())
                } else {
                    Choice::Remote(remote.clone())
                };
            }
        };
    }

    pub fn between(
        local_changes: &OutgoingChangeset,
        remote_changes: &IncomingChangeset
    ) -> Reconciliation {

        let mut result = Reconciliation {
            apply_as_incoming: vec![],
            apply_as_outgoing: vec![],
            skipped: vec![],
        };

        let local_lookup: HashMap<&str, (&Cleartext, Duration)> =
            local_changes.changes.iter().map(|(record, time)| {
                (record.id(),
                 (record,
                  time.elapsed().unwrap_or(Duration::new(0, 0))))
            }).collect();

        for (remote, remote_modified) in remote_changes.changes.iter() {

            let action = Reconciliation::reconcile_one(
                remote,
                remote_modified.duration_since(remote_changes.timestamp)
                                          .unwrap_or(Duration::new(0, 0)),
                local_lookup.get(remote.id())
            );

            match action {
                Choice::Skip => result.skipped.push(remote.id().into()),
                Choice::Remote(ct) => result.apply_as_incoming.push(ct),
                Choice::Local(ct) => result.apply_as_outgoing.push(ct),
            }
        }
        result
    }
}


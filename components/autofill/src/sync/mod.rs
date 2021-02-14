/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod address;
mod common;
pub mod credit_card;

pub(crate) use crate::db::models::Metadata;
use crate::error::Result;
use interrupt_support::Interruptee;
use serde::Deserialize;
use sync15::{Payload, ServerTimestamp};
use sync_guid::Guid;
use types::Timestamp;

// Some traits that help us abstract away much of the sync functionality.

// A trait that abstracts the *storage* implementation of the specific record
// types, and must be implemented by the concrete record owners.
// Note that it doesn't assume a SQL database or anything concrete about the
// storage, although objects implementing this trait will live only long enough
// to perform the sync "incoming" steps - ie, a transaction is likely to live
// exactly as long as this object.
trait ProcessIncomingRecordImpl {
    type Record;

    fn stage_incoming(
        &self,
        incoming: Vec<(Payload, ServerTimestamp)>,
        signal: &dyn Interruptee,
    ) -> Result<()>;

    fn fetch_incoming_states(&self) -> Result<Vec<IncomingState<Self::Record>>>;

    /// Returns a local record that has the same values as the given incoming record (with the exception
    /// of the `guid` values which should differ) that will be used as a local duplicate record for
    /// syncing.
    fn get_local_dupe(&self, incoming: &Self::Record) -> Result<Option<(Guid, Self::Record)>>;

    fn update_local_record(&self, record: Self::Record, was_merged: bool) -> Result<()>;

    fn insert_local_record(&self, record: Self::Record) -> Result<()>;

    fn change_local_guid(&self, old_guid: &Guid, new_guid: &Guid) -> Result<()>;

    fn remove_record(&self, guid: &Guid) -> Result<()>;

    fn remove_tombstone(&self, guid: &Guid) -> Result<()>;
}

// TODO: Will need new trait for outgoing.

// A trait that abstracts the functionality in the record itself.
trait SyncRecord {
    fn record_name() -> &'static str; // "addresses" or similar, for logging/debuging.
    fn id(&self) -> &Guid;
    fn metadata(&self) -> &Metadata;
    fn metadata_mut(&mut self) -> &mut Metadata;
    // Merge or fork multiple copies of the same record. The resulting record
    // might have the same guid as the inputs, meaning it was truly merged, or
    // a different guid, in which case it was forked due to conflicting changes.
    fn merge(incoming: &Self, local: &Self, mirror: &Option<Self>) -> MergeResult<Self>
    where
        Self: Sized;
}

impl Metadata {
    /// Merge the metadata from `other`, and possibly `mirror`, into `self`
    /// (which must already have valid metadata).
    /// Note that mirror being None is an edge-case and typically means first
    /// sync since a "reset" (eg, disconnecting and reconnecting.
    pub fn merge(&mut self, other: &Metadata, mirror: &Option<&Metadata>) {
        match mirror {
            Some(m) => {
                fn get_latest_time(t1: Timestamp, t2: Timestamp, t3: Timestamp) -> Timestamp {
                    std::cmp::max(t1, std::cmp::max(t2, t3))
                }
                fn get_earliest_time(t1: Timestamp, t2: Timestamp, t3: Timestamp) -> Timestamp {
                    std::cmp::min(t1, std::cmp::min(t2, t3))
                }
                self.time_created =
                    get_earliest_time(self.time_created, other.time_created, m.time_created);
                self.time_last_used =
                    get_latest_time(self.time_last_used, other.time_last_used, m.time_last_used);
                self.time_last_modified = get_latest_time(
                    self.time_last_modified,
                    other.time_last_modified,
                    m.time_last_modified,
                );

                self.times_used = m.times_used
                    + std::cmp::max(other.times_used - m.times_used, 0)
                    + std::cmp::max(self.times_used - m.times_used, 0);
            }
            None => {
                fn get_latest_time(t1: Timestamp, t2: Timestamp) -> Timestamp {
                    std::cmp::max(t1, t2)
                }
                fn get_earliest_time(t1: Timestamp, t2: Timestamp) -> Timestamp {
                    std::cmp::min(t1, t2)
                }
                self.time_created = get_earliest_time(self.time_created, other.time_created);
                self.time_last_used = get_latest_time(self.time_last_used, other.time_last_used);
                self.time_last_modified =
                    get_latest_time(self.time_last_modified, other.time_last_modified);
                // No mirror is an edge-case that almost certainly means the
                // client was disconnected and this is the first sync after
                // reconnection. So we can't really do a simple sum() of the
                // times_used values as if the disconnection was recent, it will
                // be double the expected value.
                // So we just take the largest.
                self.times_used = std::cmp::max(other.times_used, self.times_used);
            }
        }
    }
}

// Some enums that help represent what the state of local records are.
// The idea is that the actual implementations just need to tell you what
// exists and what doesn't, but don't need to implement the actual policy for
// what that means.

// An "incoming" record can be in only 2 states.
#[derive(Debug)]
enum IncomingRecord<T> {
    Record { record: T },
    Tombstone { guid: Guid },
}

// A local record can be in any of these 4 states.
#[derive(Debug)]
enum LocalRecordInfo<T> {
    Unmodified { record: T },
    Modified { record: T },
    Tombstone { guid: Guid },
    Missing,
}

// An enum for the return value from our "merge" function, which might either
// update the record, or might fork it.
#[derive(Debug)]
enum MergeResult<T> {
    Merged { merged: T },
    Forked { forked: T },
}

// This ties the 3 possible records together and is what we expect the
// implementations to put together for us.
#[derive(Debug)]
struct IncomingState<T> {
    incoming: IncomingRecord<T>,
    local: LocalRecordInfo<T>,
    // We don't have an enum for the mirror - an Option<> is fine because we
    // don't store tombstones there.
    mirror: Option<T>,
}

/// The distinct incoming sync actions to be performed for incoming records.
#[derive(Debug, PartialEq)]
enum IncomingAction<T> {
    // Remove the local record with this GUID.
    DeleteLocalRecord { guid: Guid },
    // Insert a new record.
    Insert { record: T },
    // Update an existing record. If `was_merged` was true, then the updated
    // record isn't identical to the incoming one, so needs to be flagged as
    // dirty.
    Update { record: T, was_merged: bool },
    // We forked a record because we couldn't merge it. `forked` will have
    // a new guid, while `incoming` is the unmodified version of the incoming
    // record which we need to apply.
    Fork { forked: T, incoming: T },
    // An existing record with old_guid needs to be replaced with this record.
    UpdateLocalGuid { old_guid: Guid, record: T },
    // There's a remote tombstone, but our copy of the record is dirty. The
    // remote tombstone should be replaced with this.
    ResurrectRemoteTombstone { record: T },
    // There's a local tombstone - it should be removed and replaced with this.
    ResurrectLocalTombstone { record: T },
    // Nothing to do.
    DoNothing,
}

/// Convert a IncomingState to an IncomingAction - this is where the "policy"
/// lives for when we resurrect, or merge etc.
fn plan_incoming<T: std::fmt::Debug + SyncRecord>(
    rec_impl: &dyn ProcessIncomingRecordImpl<Record = T>,
    staged_info: IncomingState<T>,
) -> Result<IncomingAction<T>> {
    log::trace!("plan_incoming: {:?}", staged_info);
    let IncomingState {
        incoming,
        local,
        mirror,
    } = staged_info;

    let state = match incoming {
        IncomingRecord::Tombstone { guid } => {
            match local {
                LocalRecordInfo::Unmodified { .. } => {
                    // Note: On desktop, when there's a local record for an incoming tombstone, a local tombstone
                    // would created. But we don't actually need to create a local tombstone here. If we did it would
                    // immediately be deleted after being uploaded to the server.
                    IncomingAction::DeleteLocalRecord { guid }
                }
                LocalRecordInfo::Modified { record } => {
                    // Incoming tombstone with local changes should cause us to "resurrect" the local.
                    // At a minimum, the implementation will need to ensure the record is marked as
                    // dirty so it's uploaded, overwriting the server's tombstone.
                    IncomingAction::ResurrectRemoteTombstone { record }
                }
                LocalRecordInfo::Tombstone {
                    guid: tombstone_guid,
                } => {
                    assert_eq!(guid, tombstone_guid);
                    IncomingAction::DoNothing
                }
                LocalRecordInfo::Missing => IncomingAction::DoNothing,
            }
        }
        IncomingRecord::Record {
            record: mut incoming_record,
        } => {
            match local {
                LocalRecordInfo::Unmodified {
                    record: local_record,
                } => {
                    // We still need to merge the metadata, but we don't reupload
                    // just for metadata changes, so don't flag the local item
                    // as dirty.
                    let metadata = incoming_record.metadata_mut();
                    metadata.merge(
                        &local_record.metadata(),
                        &mirror.as_ref().map(|m| m.metadata()),
                    );
                    // a micro-optimization here would be to `::DoNothing` if
                    // the metadata was actually identical, but this seems like
                    // an edge-case on an edge-case?
                    IncomingAction::Update {
                        record: incoming_record,
                        was_merged: false,
                    }
                }
                LocalRecordInfo::Modified {
                    record: local_record,
                } => {
                    match SyncRecord::merge(&incoming_record, &local_record, &mirror) {
                        MergeResult::Merged { merged } => {
                            // The record we save locally has material differences
                            // from the incoming one, so we are going to need to
                            // reupload it.
                            IncomingAction::Update {
                                record: merged,
                                was_merged: true,
                            }
                        }
                        MergeResult::Forked { forked } => IncomingAction::Fork {
                            forked,
                            incoming: incoming_record,
                        },
                    }
                }
                LocalRecordInfo::Tombstone { .. } => IncomingAction::ResurrectLocalTombstone {
                    record: incoming_record,
                },
                LocalRecordInfo::Missing => {
                    match rec_impl.get_local_dupe(&incoming_record)? {
                        None => IncomingAction::Insert {
                            record: incoming_record,
                        },
                        Some((old_guid, local_dupe)) => {
                            assert_ne!(incoming_record.id(), local_dupe.id());
                            // The existing item is identical except for the metadata, so
                            // we still merge that metadata.
                            let metadata = incoming_record.metadata_mut();
                            metadata.merge(
                                &local_dupe.metadata(),
                                &mirror.as_ref().map(|m| m.metadata()),
                            );
                            IncomingAction::UpdateLocalGuid {
                                old_guid,
                                record: incoming_record,
                            }
                        }
                    }
                }
            }
        }
    };
    log::trace!("plan_incoming resulted in {:?}", state);
    Ok(state)
}

/// Apply the incoming action
fn apply_incoming_action<T: std::fmt::Debug + SyncRecord>(
    rec_impl: &dyn ProcessIncomingRecordImpl<Record = T>,
    action: IncomingAction<T>,
) -> Result<()> {
    log::trace!("applying action: {:?}", action);
    match action {
        IncomingAction::Update { record, was_merged } => {
            rec_impl.update_local_record(record, was_merged)?;
        }
        IncomingAction::Fork { forked, incoming } => {
            // `forked` exists in the DB with the same guid as `incoming`, so fix that.
            rec_impl.change_local_guid(incoming.id(), forked.id())?;
            // `incoming` has the correct new guid.
            rec_impl.insert_local_record(incoming)?;
        }
        IncomingAction::Insert { record } => {
            rec_impl.insert_local_record(record)?;
        }
        IncomingAction::UpdateLocalGuid { old_guid, record } => {
            // expect record to have the new guid.
            assert_ne!(old_guid, *record.id());
            rec_impl.change_local_guid(&old_guid, record.id())?;
            // the item is identical with the item with the new guid
            // *except* for the metadata - so we still need to update, but
            // don't need to treat the item as dirty.
            rec_impl.update_local_record(record, false)?;
        }
        IncomingAction::ResurrectLocalTombstone { record } => {
            rec_impl.remove_tombstone(record.id())?;
            rec_impl.insert_local_record(record)?;
        }
        IncomingAction::ResurrectRemoteTombstone { record } => {
            // This is just "ensure local record dirty", which
            // update_local_record conveniently does.
            rec_impl.update_local_record(record, true)?;
        }
        IncomingAction::DeleteLocalRecord { guid } => {
            rec_impl.remove_record(&guid)?;
        }
        IncomingAction::DoNothing => {}
    }
    Ok(())
}

// needs a better name :) But this is how all the above ties together.
#[allow(dead_code)]
fn do_incoming<T: std::fmt::Debug + SyncRecord + for<'a> Deserialize<'a>>(
    rec_impl: &dyn ProcessIncomingRecordImpl<Record = T>,
    incoming: Vec<(Payload, ServerTimestamp)>,
    signal: &dyn Interruptee,
) -> Result<()> {
    // The first step in the "apply incoming" process for syncing autofill records.
    rec_impl.stage_incoming(incoming, signal)?;
    // 2nd step is to get "states" for each record...
    for state in rec_impl.fetch_incoming_states()? {
        signal.err_if_interrupted()?;
        // Finally get a "plan" and apply it.
        let action = plan_incoming(rec_impl, state)?;
        apply_incoming_action(rec_impl, action)?;
    }
    Ok(())
}

// Helpers for tests
#[cfg(test)]
pub mod test {
    use crate::db::{schema::create_empty_sync_temp_tables, test::new_mem_db, AutofillDb};

    pub fn new_syncable_mem_db() -> AutofillDb {
        let _ = env_logger::try_init();
        let db = new_mem_db();
        create_empty_sync_temp_tables(&db).expect("should work");
        db
    }
}

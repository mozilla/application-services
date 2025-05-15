/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod address;
mod common;
pub mod credit_card;
pub mod engine;

pub(crate) use crate::db::models::Metadata;
use crate::error::Result;
use error_support::{trace, warn};
use interrupt_support::Interruptee;
use rusqlite::Transaction;
use sync15::bso::{IncomingBso, IncomingContent, IncomingEnvelope, IncomingKind, OutgoingBso};
use sync15::ServerTimestamp;
use sync_guid::Guid;
use types::Timestamp;

// This type is used as a snazzy way to capture all unknown fields from the payload
// upon deserialization without having to work with a concrete type
type UnknownFields = serde_json::Map<String, serde_json::Value>;

// The fact that credit-card numbers are encrypted makes things a little tricky
// for sync in various ways - and one non-obvious way is that the tables that
// store sync payloads can't just store them directly as they are not encrypted
// in that form.
// ie, in the database, an address record's "payload" column looks like:
// > '{"entry":{"address-level1":"VIC", "street-address":"2/25 Somewhere St","timeCreated":1497567116554, "version":1},"id":"29ac67adae7d"}'
// or a tombstone: '{"deleted":true,"id":"6544992973e6"}'
// > (Note a number of fields have been removed from 'entry' for clarity)
// and in the database a credit-card's "payload" looks like:
// > 'eyJhbGciOiJkaXIiLCJlbmMiOiJBMjU2R0NNIn0..<snip>-<snip>.<snip lots more>'
// > while a tombstone here remains encrypted but has the 'deleted' entry after decryption.
// (Note also that the address entry, and the decrypted credit-card json both have an "id" in
// the JSON, but we ignore that when deserializing and will stop persisting that soon)

// Some traits that help us abstract away much of the sync functionality.

// A trait that abstracts the *storage* implementation of the specific record
// types, and must be implemented by the concrete record owners.
// Note that it doesn't assume a SQL database or anything concrete about the
// storage, although objects implementing this trait will live only long enough
// to perform the sync "incoming" steps - ie, a transaction is likely to live
// exactly as long as this object.
// XXX - *sob* - although each method has a `&Transaction` param, which in
// theory could be avoided if the concrete impls could keep the ref (ie, if
// it was held behind `self`), but markh failed to make this work due to
// lifetime woes.
pub trait ProcessIncomingRecordImpl {
    type Record;

    fn stage_incoming(
        &self,
        tx: &Transaction<'_>,
        incoming: Vec<IncomingBso>,
        signal: &dyn Interruptee,
    ) -> Result<()>;

    /// Finish the incoming phase. This will typically caused staged records
    // to be written to the mirror.
    fn finish_incoming(&self, tx: &Transaction<'_>) -> Result<()>;

    fn fetch_incoming_states(
        &self,
        tx: &Transaction<'_>,
    ) -> Result<Vec<IncomingState<Self::Record>>>;

    /// Returns a local record that has the same values as the given incoming record (with the exception
    /// of the `guid` values which should differ) that will be used as a local duplicate record for
    /// syncing.
    fn get_local_dupe(
        &self,
        tx: &Transaction<'_>,
        incoming: &Self::Record,
    ) -> Result<Option<Self::Record>>;

    fn update_local_record(
        &self,
        tx: &Transaction<'_>,
        record: Self::Record,
        was_merged: bool,
    ) -> Result<()>;

    fn insert_local_record(&self, tx: &Transaction<'_>, record: Self::Record) -> Result<()>;

    fn change_record_guid(
        &self,
        tx: &Transaction<'_>,
        old_guid: &Guid,
        new_guid: &Guid,
    ) -> Result<()>;

    fn remove_record(&self, tx: &Transaction<'_>, guid: &Guid) -> Result<()>;

    fn remove_tombstone(&self, tx: &Transaction<'_>, guid: &Guid) -> Result<()>;
}

pub trait ProcessOutgoingRecordImpl {
    type Record;

    fn fetch_outgoing_records(&self, tx: &Transaction<'_>) -> anyhow::Result<Vec<OutgoingBso>>;

    fn finish_synced_items(
        &self,
        tx: &Transaction<'_>,
        records_synced: Vec<Guid>,
    ) -> anyhow::Result<()>;
}

// A trait that abstracts the functionality in the record itself.
pub trait SyncRecord {
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
    pub fn merge(&mut self, other: &Metadata, mirror: Option<&Metadata>) {
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

// A local record can be in any of these 5 states.
#[derive(Debug)]
enum LocalRecordInfo<T> {
    Unmodified { record: T },
    Modified { record: T },
    // encrypted data was scrubbed from the local record and needs to be resynced from the server
    Scrubbed { record: T },
    Tombstone { guid: Guid },
    Missing,
}

// An enum for the return value from our "merge" function, which might either
// update the record, or might fork it.
#[derive(Debug)]
pub enum MergeResult<T> {
    Merged { merged: T },
    Forked { forked: T },
}

// This ties the 3 possible records together and is what we expect the
// implementations to put together for us.
#[derive(Debug)]
pub struct IncomingState<T> {
    incoming: IncomingContent<T>,
    local: LocalRecordInfo<T>,
    // We don't have an enum for the mirror - an Option<> is fine because
    // although we do store tombstones there, we ignore them when reconciling
    // (ie, we ignore tombstones in the mirror)
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
    tx: &Transaction<'_>,
    staged_info: IncomingState<T>,
) -> Result<IncomingAction<T>> {
    trace!("plan_incoming: {:?}", staged_info);
    let IncomingState {
        incoming,
        local,
        mirror,
    } = staged_info;

    let state = match incoming.kind {
        IncomingKind::Tombstone => {
            match local {
                LocalRecordInfo::Unmodified { .. } | LocalRecordInfo::Scrubbed { .. } => {
                    // Note: On desktop, when there's a local record for an incoming tombstone, a local tombstone
                    // would created. But we don't actually need to create a local tombstone here. If we did it would
                    // immediately be deleted after being uploaded to the server.
                    IncomingAction::DeleteLocalRecord {
                        guid: incoming.envelope.id,
                    }
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
                    assert_eq!(incoming.envelope.id, tombstone_guid);
                    IncomingAction::DoNothing
                }
                LocalRecordInfo::Missing => IncomingAction::DoNothing,
            }
        }
        IncomingKind::Content(mut incoming_record) => {
            match local {
                LocalRecordInfo::Unmodified {
                    record: local_record,
                }
                | LocalRecordInfo::Scrubbed {
                    record: local_record,
                } => {
                    // The local record was either unmodified, or scrubbed of its encrypted data.
                    // Either way we want to:
                    //   - Merge the metadata
                    //   - Update the local record using data from the server
                    //   - Don't flag the local item as dirty.  We don't want to reupload for just
                    //     metadata changes.
                    let metadata = incoming_record.metadata_mut();
                    metadata.merge(
                        local_record.metadata(),
                        mirror.as_ref().map(|m| m.metadata()),
                    );
                    // a micro-optimization here would be to `::DoNothing` if
                    // the metadata was actually identical and the local data wasn't scrubbed, but
                    // this seems like an edge-case on an edge-case?
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
                    match rec_impl.get_local_dupe(tx, &incoming_record)? {
                        None => IncomingAction::Insert {
                            record: incoming_record,
                        },
                        Some(local_dupe) => {
                            // local record is missing but we found a dupe - so
                            // the dupe must have a different guid (or we wouldn't
                            // consider the local record missing!)
                            assert_ne!(incoming_record.id(), local_dupe.id());
                            // The existing item is identical except for the metadata, so
                            // we still merge that metadata.
                            let metadata = incoming_record.metadata_mut();
                            metadata.merge(
                                local_dupe.metadata(),
                                mirror.as_ref().map(|m| m.metadata()),
                            );
                            IncomingAction::UpdateLocalGuid {
                                old_guid: local_dupe.id().clone(),
                                record: incoming_record,
                            }
                        }
                    }
                }
            }
        }
        IncomingKind::Malformed => {
            warn!("skipping incoming record: {}", incoming.envelope.id);
            IncomingAction::DoNothing
        }
    };
    trace!("plan_incoming resulted in {:?}", state);
    Ok(state)
}

/// Apply the incoming action
fn apply_incoming_action<T: std::fmt::Debug + SyncRecord>(
    rec_impl: &dyn ProcessIncomingRecordImpl<Record = T>,
    tx: &Transaction<'_>,
    action: IncomingAction<T>,
) -> Result<()> {
    trace!("applying action: {:?}", action);
    match action {
        IncomingAction::Update { record, was_merged } => {
            rec_impl.update_local_record(tx, record, was_merged)?;
        }
        IncomingAction::Fork { forked, incoming } => {
            // `forked` exists in the DB with the same guid as `incoming`, so fix that.
            // change_record_guid will also update the mirror (if it exists) to prevent
            // the server from overriding the forked mirror record (and losing any unknown fields)
            rec_impl.change_record_guid(tx, incoming.id(), forked.id())?;
            // `incoming` has the correct new guid.
            rec_impl.insert_local_record(tx, incoming)?;
        }
        IncomingAction::Insert { record } => {
            rec_impl.insert_local_record(tx, record)?;
        }
        IncomingAction::UpdateLocalGuid { old_guid, record } => {
            // expect record to have the new guid.
            assert_ne!(old_guid, *record.id());
            rec_impl.change_record_guid(tx, &old_guid, record.id())?;
            // the item is identical with the item with the new guid
            // *except* for the metadata - so we still need to update, but
            // don't need to treat the item as dirty.
            rec_impl.update_local_record(tx, record, false)?;
        }
        IncomingAction::ResurrectLocalTombstone { record } => {
            rec_impl.remove_tombstone(tx, record.id())?;
            rec_impl.insert_local_record(tx, record)?;
        }
        IncomingAction::ResurrectRemoteTombstone { record } => {
            // This is just "ensure local record dirty", which
            // update_local_record conveniently does.
            rec_impl.update_local_record(tx, record, true)?;
        }
        IncomingAction::DeleteLocalRecord { guid } => {
            rec_impl.remove_record(tx, &guid)?;
        }
        IncomingAction::DoNothing => {}
    }
    Ok(())
}

// Helpers for tests
#[cfg(test)]
mod tests; // pull in our integration tests

// and a module for unit test utilities.
#[cfg(test)]
pub mod test {
    use crate::db::{schema::create_empty_sync_temp_tables, test::new_mem_db, AutofillDb};

    pub fn new_syncable_mem_db() -> AutofillDb {
        error_support::init_for_tests();
        let db = new_mem_db();
        create_empty_sync_temp_tables(&db).expect("should work");
        db
    }
}

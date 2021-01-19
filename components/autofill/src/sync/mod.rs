/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod address;
// pub mod credit_card;

use crate::error::Result;
use interrupt_support::Interruptee;
use rusqlite::Connection;
use sync_guid::Guid as SyncGuid;

// Some enums that help represent what the state of local records are.
// The idea is that the actual implementations just need to tell you what
// exists and what doesn't, but don't need to implement the actual policy for
// what that means.

// An "incoming" record can be in only 2 states.
enum IncomingRecordInfo<T> {
    Record { record: T },
    Tombstone { guid: SyncGuid },
}

// A local record can be in any of these 4 states.
enum LocalRecordInfo<T> {
    Unmodified { record: T },
    Modified { record: T },
    Tombstone { guid: SyncGuid },
    Missing,
}

// This ties the 3 possible records together and is what we expect the
// implementations to put together for us.
pub struct IncomingState<T> {
    incoming: IncomingRecordInfo<T>,
    local: LocalRecordInfo<T>,
    // We don't have an enum for the mirror - an Option<> is fine because we
    // don't store tombstones there.
    mirror: Option<T>,
}

/// Convert a IncomingState to an IncomingAction - this is where the "policy"
/// lives for when we resurrect, or merge etc.
fn plan_incoming<T>(
    conn: &Connection,
    rec_impl: &dyn RecordImpl<Record = T>,
    staged_info: IncomingState<T>,
) -> Result<IncomingAction<T>> {
    let IncomingState {
        incoming,
        local,
        mirror,
    } = staged_info;

    let state = match incoming {
        IncomingRecordInfo::Tombstone { guid } => {
            match local {
                LocalRecordInfo::Unmodified { .. } => {
                    // Note: On desktop, when there's a local record for an incoming tombstone, a local tombstone
                    // would created. But we don't actually need to create a local tombstone here. If we did it would
                    // immediately be deleted after being uploaded to the server.
                    IncomingAction::DeleteLocalRecord { guid }
                }
                LocalRecordInfo::Modified { record } => {
                    // Incoming tombstone with local changes should cause us to "resurrect" the local. It's
                    // likely that the implementation of this is to do nothing, but it's better to be explicit.
                    IncomingAction::ResurrectLocalTombstone { record }
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
        IncomingRecordInfo::Record {
            record: incoming_record,
        } => {
            match local {
                LocalRecordInfo::Unmodified {
                    record: local_record,
                } => {
                    // We still merge so the metadata is up-to-date.
                    let record = rec_impl.merge(&incoming_record, &local_record, &mirror);
                    IncomingAction::Update { record }
                }
                LocalRecordInfo::Modified {
                    record: local_record,
                } => {
                    let merged = rec_impl.merge(&incoming_record, &local_record, &mirror);
                    IncomingAction::Update { record: merged }
                }
                LocalRecordInfo::Tombstone { .. } => IncomingAction::ResurrectLocalTombstone {
                    record: incoming_record,
                },
                LocalRecordInfo::Missing => {
                    match rec_impl.get_local_dupe(conn, &incoming_record)? {
                        None => IncomingAction::Insert {
                            record: incoming_record,
                        },
                        Some((old_guid, local_dupe)) => {
                            // *sob* - need guid fetching in the trait??? assert_ne!(incoming_record.guid, local_dupe.guid);
                            IncomingAction::UpdateLocalGuid {
                                old_guid,
                                record: rec_impl.merge(&incoming_record, &local_dupe, &mirror),
                            }
                        }
                    }
                }
            }
        }
    };
    Ok(state)
}

/// The distinct incoming sync actions to be performed for incoming records.
#[derive(Debug, PartialEq)]
pub enum IncomingAction<T> {
    // Remove the local record with this GUID.
    DeleteLocalRecord { guid: SyncGuid },
    // Insert a new record.
    Insert { record: T },
    // Update an existing record.
    Update { record: T },
    // An existing record with old_guid needs to be replaced with this record.
    UpdateLocalGuid { old_guid: SyncGuid, record: T },
    // There's a local tombstone - it should be removed and replaced with this.
    ResurrectLocalTombstone { record: T },
    // Nothing to do.
    DoNothing,
}

// A trait that abstracts the implementation of the specific record types, and
// must be implemented by the concrete record owners.
trait RecordImpl {
    type Record;

    fn fetch_incoming_states(&self, conn: &Connection) -> Result<Vec<IncomingState<Self::Record>>>;

    // Merge multiple records into 1.
    fn merge(
        &self,
        incoming: &Self::Record,
        local: &Self::Record,
        mirror: &Option<Self::Record>,
    ) -> Self::Record;

    /// Returns a local record that has the same values as the given incoming record (with the exception
    /// of the `guid` values which should differ) that will be used as a local duplicate record for
    /// syncing.
    fn get_local_dupe(
        &self,
        conn: &Connection,
        incoming: &Self::Record,
    ) -> Result<Option<(SyncGuid, Self::Record)>>;

    // Apply a specific action
    fn apply_action(&self, conn: &Connection, action: IncomingAction<Self::Record>) -> Result<()>;

    // Will need new stuff for, "finish incoming" and all outgoing.
}

// needs a better name :) But this is how all the above ties together.
#[allow(dead_code)]
fn do_incoming<T>(
    conn: &Connection,
    rec_impl: &dyn RecordImpl<Record = T>,
    signal: &dyn Interruptee,
) -> Result<()> {
    let states = rec_impl.fetch_incoming_states(conn)?;
    for state in states {
        signal.err_if_interrupted()?;
        let action = plan_incoming(conn, rec_impl, state)?;
        rec_impl.apply_action(conn, action)?;
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

#[cfg(test)]
mod tests {
    use super::test::new_syncable_mem_db;
    use super::*;

    struct TestImpl {}

    impl RecordImpl for TestImpl {
        type Record = i32;

        fn fetch_incoming_states(
            &self,
            _conn: &Connection,
        ) -> Result<Vec<IncomingState<Self::Record>>> {
            unreachable!();
        }

        fn merge(
            &self,
            _incoming: &Self::Record,
            _local: &Self::Record,
            _mirror: &Option<Self::Record>,
        ) -> Self::Record {
            unreachable!();
        }

        fn get_local_dupe(
            &self,
            _conn: &Connection,
            _incoming: &Self::Record,
        ) -> Result<Option<(SyncGuid, Self::Record)>> {
            Ok(None)
        }

        // Apply a specific action
        fn apply_action(
            &self,
            _conn: &Connection,
            _action: IncomingAction<Self::Record>,
        ) -> Result<()> {
            unreachable!();
        }
    }
    #[test]
    fn test_plan_incoming() -> Result<()> {
        let conn = new_syncable_mem_db();
        let testimpl = TestImpl {};
        let guid = SyncGuid::random();
        // We just use an int for <T> here, hence the magic 0
        let state = IncomingState {
            incoming: IncomingRecordInfo::Record { record: 0 },
            local: LocalRecordInfo::Missing,
            mirror: None,
        };
        assert_eq!(
            plan_incoming(&conn, &testimpl, state)?,
            IncomingAction::Insert { record: 0 }
        );

        // Incoming tombstone with an unmodified local record deletes the local record.
        let state = IncomingState {
            incoming: IncomingRecordInfo::Tombstone { guid: guid.clone() },
            local: LocalRecordInfo::Unmodified { record: 0 },
            mirror: None,
        };
        assert_eq!(
            plan_incoming(&conn, &testimpl, state)?,
            IncomingAction::DeleteLocalRecord { guid: guid.clone() }
        );

        // Incoming with no matching local record does nothing/
        let state = IncomingState {
            incoming: IncomingRecordInfo::Tombstone { guid },
            local: LocalRecordInfo::Missing,
            mirror: None,
        };
        assert_eq!(
            plan_incoming(&conn, &testimpl, state)?,
            IncomingAction::DoNothing
        );

        // TODO - the rest
        Ok(())
    }
}

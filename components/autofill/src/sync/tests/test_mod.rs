/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::super::test::new_syncable_mem_db;
use super::super::*;

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
        incoming: &Self::Record,
        local: &Self::Record,
        _mirror: &Option<Self::Record>,
    ) -> MergeResult<Self::Record> {
        // If the records are actually identical, or even, we say we merged.
        if incoming == local {
            MergeResult::Merged { merged: *incoming }
        } else {
            MergeResult::Forked {
                forked: incoming + local,
            }
        }
    }

    fn merge_metadata(
        &self,
        _result: &mut Self::Record,
        _other: &Self::Record,
        _mirror: &Option<Self::Record>,
    ) {
        // do nothing.
    }

    fn get_local_dupe(
        &self,
        _conn: &Connection,
        incoming: &Self::Record,
    ) -> Result<Option<(SyncGuid, Self::Record)>> {
        // For the sake of this test, we pretend even numbers have a dupe.
        Ok(if incoming % 2 == 0 {
            Some((SyncGuid::random(), *incoming))
        } else {
            None
        })
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
fn test_plan_incoming_record() -> Result<()> {
    let conn = new_syncable_mem_db();
    let testimpl = TestImpl {};
    let guid = SyncGuid::random();
    // We just use an int for <T> here, hence the magic 0

    // LocalRecordInfo::UnModified - update the local with the incoming.
    let state = IncomingState {
        incoming: IncomingRecordInfo::Record { record: 0 },
        local: LocalRecordInfo::Unmodified { record: 1 },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&conn, &testimpl, state)?,
        IncomingAction::Update {
            record: 0,
            was_merged: false
        }
    );

    // LocalRecordInfo::Modified - but it turns out they are identical.
    let state = IncomingState {
        incoming: IncomingRecordInfo::Record { record: 0 },
        local: LocalRecordInfo::Modified { record: 0 },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&conn, &testimpl, state)?,
        IncomingAction::Update {
            record: 0,
            was_merged: true
        }
    );

    // LocalRecordInfo::Modified and they need to be "forked"
    let state = IncomingState {
        incoming: IncomingRecordInfo::Record { record: 1 },
        local: LocalRecordInfo::Modified { record: 2 },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&conn, &testimpl, state)?,
        IncomingAction::Fork {
            forked: 3,
            incoming: 1
        }
    );

    // LocalRecordInfo::Tombstone - the local tombstone needs to be
    // resurrected.
    let state = IncomingState {
        incoming: IncomingRecordInfo::Record { record: 1 },
        local: LocalRecordInfo::Tombstone { guid },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&conn, &testimpl, state)?,
        IncomingAction::ResurrectLocalTombstone { record: 1 }
    );

    // LocalRecordInfo::Missing and a local dupe (even numbers will dupe)
    let state = IncomingState {
        incoming: IncomingRecordInfo::Record { record: 0 },
        local: LocalRecordInfo::Missing,
        mirror: None,
    };
    assert!(matches!(
        plan_incoming(&conn, &testimpl, state)?,
        IncomingAction::UpdateLocalGuid { record: 0, .. }
    ));

    // LocalRecordInfo::Missing and no dupe - it's an insert.
    let state = IncomingState {
        incoming: IncomingRecordInfo::Record { record: 1 },
        local: LocalRecordInfo::Missing,
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&conn, &testimpl, state)?,
        IncomingAction::Insert { record: 1 }
    );
    Ok(())
}

#[test]
fn test_plan_incoming_tombstone() -> Result<()> {
    let conn = new_syncable_mem_db();
    let testimpl = TestImpl {};
    let guid = SyncGuid::random();
    // We just use an int for <T> here, hence the magic 0

    // LocalRecordInfo::Modified
    // Incoming tombstone with an modified local record deletes the local record.
    let state = IncomingState {
        incoming: IncomingRecordInfo::Tombstone { guid: guid.clone() },
        local: LocalRecordInfo::Unmodified { record: 0 },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&conn, &testimpl, state)?,
        IncomingAction::DeleteLocalRecord { guid: guid.clone() }
    );

    // LocalRecordInfo::Modified
    // Incoming tombstone with an modified local record keeps the local record.
    let state = IncomingState {
        incoming: IncomingRecordInfo::Tombstone { guid: guid.clone() },
        local: LocalRecordInfo::Modified { record: 0 },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&conn, &testimpl, state)?,
        IncomingAction::ResurrectRemoteTombstone { record: 0 }
    );

    // LocalRecordInfo::Tombstone
    // Local tombstone and incoming tombstone == DoNothing.
    let state = IncomingState {
        incoming: IncomingRecordInfo::Tombstone { guid: guid.clone() },
        local: LocalRecordInfo::Tombstone { guid: guid.clone() },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&conn, &testimpl, state)?,
        IncomingAction::DoNothing
    );

    // LocalRecordInfo::Missing
    // Incoming tombstone and no local record == DoNothing.
    let state = IncomingState {
        incoming: IncomingRecordInfo::Tombstone { guid },
        local: LocalRecordInfo::Missing,
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&conn, &testimpl, state)?,
        IncomingAction::DoNothing
    );
    Ok(())
}

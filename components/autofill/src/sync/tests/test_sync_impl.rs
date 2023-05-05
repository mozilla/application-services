/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

// Tests the implementation of the sync code using a custom implementation
// of what a "record" means.

use super::super::test::new_syncable_mem_db;
use super::super::*;
use sync_guid::Guid as SyncGuid;

#[derive(Default, Debug, Clone, PartialEq)]
struct TestStruct {
    pub guid: Guid,
    pub value: i32,
    pub metadata: Metadata,
}

impl TestStruct {
    fn new(guid: &SyncGuid, value: i32) -> Self {
        TestStruct {
            guid: guid.clone(),
            value,
            ..Default::default()
        }
    }
}

struct TestImpl {}

impl ProcessIncomingRecordImpl for TestImpl {
    type Record = TestStruct;

    fn stage_incoming(
        &self,
        _tx: &Transaction<'_>,
        _incoming: Vec<IncomingBso>,
        _signal: &dyn Interruptee,
    ) -> Result<()> {
        unreachable!();
    }

    fn finish_incoming(&self, _tx: &Transaction<'_>) -> Result<()> {
        unreachable!();
    }

    fn fetch_incoming_states(
        &self,
        _tx: &Transaction<'_>,
    ) -> Result<Vec<IncomingState<Self::Record>>> {
        unreachable!();
    }

    fn get_local_dupe(
        &self,
        _tx: &Transaction<'_>,
        incoming: &Self::Record,
    ) -> Result<Option<Self::Record>> {
        // For the sake of this test, we pretend even numbers have a dupe.
        Ok(if incoming.value % 2 == 0 {
            Some(TestStruct::new(&SyncGuid::random(), incoming.value))
        } else {
            None
        })
    }

    fn update_local_record(
        &self,
        _tx: &Transaction<'_>,
        _new_record: Self::Record,
        _flag_as_changed: bool,
    ) -> Result<()> {
        unreachable!();
    }

    fn insert_local_record(&self, _tx: &Transaction<'_>, _new_record: Self::Record) -> Result<()> {
        unreachable!();
    }

    fn change_record_guid(
        &self,
        _tx: &Transaction<'_>,
        _old_guid: &SyncGuid,
        _new_guid: &SyncGuid,
    ) -> Result<()> {
        unreachable!();
    }

    fn remove_record(&self, _tx: &Transaction<'_>, _guid: &SyncGuid) -> Result<()> {
        unreachable!();
    }

    fn remove_tombstone(&self, _tx: &Transaction<'_>, _guid: &SyncGuid) -> Result<()> {
        unreachable!();
    }

    /*
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


        // Apply a specific action
        fn apply_action(
            &self,
            _conn: &Connection,
            _action: IncomingAction<Self::Record>,
        ) -> Result<()> {
            unreachable!();
        }
    */
}

impl SyncRecord for TestStruct {
    fn record_name() -> &'static str {
        "TestStruct"
    }

    fn id(&self) -> &Guid {
        &self.guid
    }

    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }

    /// Performs a three-way merge between an incoming, local, and mirror record.
    /// If a merge cannot be successfully completed (ie, if we find the same
    /// field has changed both locally and remotely since the last sync), the
    /// local record data is returned with a new guid and updated sync metadata.
    /// Note that mirror being None is an edge-case and typically means first
    /// sync since a "reset" (eg, disconnecting and reconnecting.
    #[allow(clippy::cognitive_complexity)] // Looks like clippy considers this after macro-expansion...
    fn merge(incoming: &Self, local: &Self, _mirror: &Option<Self>) -> MergeResult<Self> {
        // If the records are identical we say we merged.
        if incoming.value == local.value {
            MergeResult::Merged {
                merged: TestStruct::new(&incoming.guid, incoming.value),
            }
        } else {
            MergeResult::Forked {
                forked: TestStruct::new(&SyncGuid::random(), incoming.value + local.value),
            }
        }
    }
}

fn new_test_incoming_content(t: TestStruct) -> IncomingContent<TestStruct> {
    IncomingContent {
        envelope: IncomingEnvelope {
            id: t.guid.clone(),
            modified: ServerTimestamp::default(),
            sortindex: None,
            ttl: None,
        },
        kind: IncomingKind::Content(t),
    }
}

fn new_test_incoming_tombstone(guid: SyncGuid) -> IncomingContent<TestStruct> {
    IncomingContent {
        envelope: IncomingEnvelope {
            id: guid,
            modified: ServerTimestamp::default(),
            sortindex: None,
            ttl: None,
        },
        kind: IncomingKind::Tombstone,
    }
}

#[test]
fn test_plan_incoming_record() -> Result<()> {
    let conn = new_syncable_mem_db();
    let tx = conn.unchecked_transaction()?;
    let testimpl = TestImpl {};
    let guid = SyncGuid::random();
    // LocalRecordInfo::UnModified - update the local with the incoming.
    let state = IncomingState {
        incoming: new_test_incoming_content(TestStruct::new(&guid, 0)),
        local: LocalRecordInfo::Unmodified {
            record: TestStruct::new(&guid, 1),
        },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&testimpl, &tx, state)?,
        IncomingAction::Update {
            record: TestStruct::new(&guid, 0),
            was_merged: false
        }
    );

    // LocalRecordInfo::Scrubbed - update the local with the incoming.
    let state = IncomingState {
        incoming: new_test_incoming_content(TestStruct::new(&guid, 0)),
        local: LocalRecordInfo::Scrubbed {
            record: TestStruct::new(&guid, 1),
        },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&testimpl, &tx, state)?,
        IncomingAction::Update {
            record: TestStruct::new(&guid, 0),
            was_merged: false
        }
    );

    // LocalRecordInfo::Modified - but it turns out they are identical.
    let state = IncomingState {
        incoming: new_test_incoming_content(TestStruct::new(&guid, 0)),
        local: LocalRecordInfo::Modified {
            record: TestStruct::new(&guid, 0),
        },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&testimpl, &tx, state)?,
        IncomingAction::Update {
            record: TestStruct::new(&guid, 0),
            was_merged: true
        }
    );

    // LocalRecordInfo::Modified and they need to be "forked"
    let state = IncomingState {
        incoming: new_test_incoming_content(TestStruct::new(&guid, 1)),
        local: LocalRecordInfo::Modified {
            record: TestStruct::new(&guid, 2),
        },
        mirror: None,
    };

    match plan_incoming(&testimpl, &tx, state)? {
        IncomingAction::Fork { forked, incoming } => {
            assert_eq!(incoming, TestStruct::new(&guid, 1));
            // `forked` has a new guid, so can't check the entire struct.
            assert_eq!(forked.value, 3);
        }
        _ => unreachable!(),
    }

    // LocalRecordInfo::Tombstone - the local tombstone needs to be
    // resurrected.
    let state = IncomingState {
        incoming: new_test_incoming_content(TestStruct::new(&guid, 1)),
        local: LocalRecordInfo::Tombstone { guid: guid.clone() },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&testimpl, &tx, state)?,
        IncomingAction::ResurrectLocalTombstone {
            record: TestStruct::new(&guid, 1)
        }
    );

    // LocalRecordInfo::Missing and a local dupe (even numbers will dupe)
    let state = IncomingState {
        incoming: new_test_incoming_content(TestStruct::new(&guid, 0)),
        local: LocalRecordInfo::Missing,
        mirror: None,
    };
    assert!(matches!(
        plan_incoming(&testimpl, &tx, state)?,
        IncomingAction::UpdateLocalGuid { .. }
    ));

    // LocalRecordInfo::Missing and no dupe - it's an insert.
    let state = IncomingState {
        incoming: new_test_incoming_content(TestStruct::new(&guid, 1)),
        local: LocalRecordInfo::Missing,
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&testimpl, &tx, state)?,
        IncomingAction::Insert {
            record: TestStruct::new(&guid, 1)
        }
    );
    Ok(())
}

#[test]
fn test_plan_incoming_tombstone() -> Result<()> {
    let conn = new_syncable_mem_db();
    let tx = conn.unchecked_transaction()?;
    let testimpl = TestImpl {};
    let guid = SyncGuid::random();

    // LocalRecordInfo::Modified
    // Incoming tombstone with an modified local record deletes the local record.
    let state = IncomingState {
        incoming: new_test_incoming_tombstone(guid.clone()),
        local: LocalRecordInfo::Unmodified {
            record: TestStruct::new(&guid, 0),
        },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&testimpl, &tx, state)?,
        IncomingAction::DeleteLocalRecord { guid: guid.clone() }
    );

    // LocalRecordInfo::Modified
    // Incoming tombstone with an modified local record keeps the local record.
    let state = IncomingState {
        incoming: new_test_incoming_tombstone(guid.clone()),
        local: LocalRecordInfo::Modified {
            record: TestStruct::new(&guid, 0),
        },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&testimpl, &tx, state)?,
        IncomingAction::ResurrectRemoteTombstone {
            record: TestStruct::new(&guid, 0)
        }
    );
    // LocalRecordInfo::Tombstone
    // Local tombstone and incoming tombstone == DoNothing.
    let state = IncomingState {
        incoming: new_test_incoming_tombstone(guid.clone()),
        local: LocalRecordInfo::Tombstone { guid: guid.clone() },
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&testimpl, &tx, state)?,
        IncomingAction::DoNothing
    );

    // LocalRecordInfo::Missing
    // Incoming tombstone and no local record == DoNothing.
    let state = IncomingState {
        incoming: new_test_incoming_tombstone(guid),
        local: LocalRecordInfo::Missing,
        mirror: None,
    };
    assert_eq!(
        plan_incoming(&testimpl, &tx, state)?,
        IncomingAction::DoNothing
    );
    Ok(())
}

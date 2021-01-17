/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod address;
// pub mod credit_card;

use serde::Serialize;
use serde_derive::*;
use sync_guid::Guid as SyncGuid;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Record<T> {
    #[serde(rename = "id", default)]
    pub guid: SyncGuid,

    #[serde(flatten)]
    data: T,
}

impl<T> Record<T> {
    fn new(guid: SyncGuid, data: T) -> Record<T> {
        Record { guid, data }
    }
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

/// The distinct states of records to be synced which determine the `IncomingAction` to be taken.
#[derive(Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum IncomingState<T> {
    // Only the incoming record exists. An associated local or mirror record doesn't exist.
    IncomingOnly {
        guid: SyncGuid,
        incoming: T,
    },
    // The incoming record is a tombstone.
    IncomingTombstone {
        guid: SyncGuid,
        local: Option<T>,
        has_local_changes: bool,
        has_local_tombstone: bool,
    },
    // The incoming record has an associated local record.
    HasLocal {
        guid: SyncGuid,
        incoming: T,
        merged: Record<T>,
        has_local_changes: bool,
    },
    // The incoming record doesn't have an associated local record with the same GUID.
    // A local record with the same data but a different GUID has been located.
    HasLocalDupe {
        guid: SyncGuid,
        dupe_guid: SyncGuid,
        merged: Record<T>,
    },
    // The incoming record doesn't have an associated local or local duplicate record but does
    // have a local tombstone.
    NonDeletedIncoming {
        guid: SyncGuid,
        incoming: T,
    },
}

/// The distinct incoming sync actions to be preformed for incoming records.
#[derive(Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum IncomingAction<T> {
    DeleteLocalRecord {
        guid: SyncGuid,
    },
    TakeMergedRecord {
        new_record: Record<T>,
    },
    UpdateLocalGuid {
        old_guid: SyncGuid,
        dupe_guid: SyncGuid,
        new_record: Record<T>,
    },
    TakeRemote {
        new_record: Record<T>,
    },
    DeleteLocalTombstone {
        remote_record: Record<T>,
    },
    DoNothing,
}

/// Given an `IncomingState` returns the `IncomingAction` that should be performed.
pub fn plan_incoming<T>(s: IncomingState<T>) -> IncomingAction<T> {
    match s {
        IncomingState::IncomingOnly { guid, incoming } => IncomingAction::TakeRemote {
            new_record: Record::<T>::new(guid, incoming),
        },
        IncomingState::IncomingTombstone {
            guid,
            local,
            has_local_changes,
            has_local_tombstone,
        } => match local {
            Some(_) => {
                // Note: On desktop, when there's a local record for an incoming tombstone, a local tombstone
                // would created. But we don't actually need to create a local tombstone here. If we did it would
                // immediately be deleted after being uploaded to the server.

                if has_local_changes || has_local_tombstone {
                    IncomingAction::DoNothing
                } else {
                    IncomingAction::DeleteLocalRecord {
                        guid: SyncGuid::new(&guid),
                    }
                }
            }
            None => IncomingAction::DoNothing,
        },
        IncomingState::HasLocal {
            guid,
            incoming,
            merged,
            has_local_changes,
        } => match has_local_changes {
            true => IncomingAction::TakeMergedRecord { new_record: merged },
            false => IncomingAction::TakeRemote {
                new_record: Record::<T>::new(guid, incoming),
            },
        },
        IncomingState::HasLocalDupe {
            guid,
            dupe_guid,
            merged,
        } => IncomingAction::UpdateLocalGuid {
            old_guid: guid,
            dupe_guid,
            new_record: merged,
        },
        IncomingState::NonDeletedIncoming { guid, incoming } => {
            IncomingAction::DeleteLocalTombstone {
                remote_record: Record::<T>::new(guid, incoming),
            }
        }
    }
}

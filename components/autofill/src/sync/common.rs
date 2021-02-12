/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

// This contains sync functionality we've managed to share between addresses
// and credit-cards. It's not "generic" in the way that traits are, it's
// literally just code we can share.
// For example, this code doesn't abstract storage away - it knows we are
// using a sql database and knows that the schemas for addresses and cards are
// very similar.

use crate::error::*;
use crate::sync::{IncomingRecordInfo, IncomingState, LocalRecordInfo, SyncRecord};
use interrupt_support::Interruptee;
use rusqlite::{types::ToSql, Connection};
use sql_support::ConnExt;
use sync15::ServerTimestamp;
use sync_guid::Guid;

/// Stages incoming records (excluding incoming tombstones) in preparation for
/// applying incoming changes for the syncing autofill records.
pub(super) fn common_stage_incoming_records<T: SyncRecord, F>(
    conn: &Connection,
    table_name: &str,
    columns: &str,
    records: Vec<(T, ServerTimestamp)>,
    signal: &dyn Interruptee,
    param_maker: F,
) -> Result<()>
where
    F: Fn(&T) -> Vec<&dyn ToSql>,
{
    log::info!(
        "staging {} incoming records into {}",
        records.len(),
        table_name
    );
    // chunking is a bit of a PITA given this is shared between things with
    // different param counts, and the record counts should be low enough
    // that it doesn't matter, so one record at a time...
    for (record, _) in records {
        signal.err_if_interrupted()?;
        let params = param_maker(&record);
        let sql = format!(
            "INSERT OR REPLACE INTO temp.{table_name} (
                    {columns}
                ) VALUES {values}",
            table_name = table_name,
            columns = columns,
            values = sql_support::repeat_multi_values(1, params.len())
        );
        conn.execute(&sql, &params)?;
    }
    Ok(())
}

/// Stages incoming tombstones (excluding incoming records) in preparation for
/// applying incoming changes for the syncing autofill records.
pub(super) fn common_stage_incoming_tombstones<T: SyncRecord>(
    conn: &Connection,
    table_name: &str,
    incoming_tombstones: Vec<(T, ServerTimestamp)>,
    signal: &dyn Interruptee,
) -> Result<()> {
    log::info!(
        "staging {} incoming tombstones into {}",
        incoming_tombstones.len(),
        table_name
    );
    sql_support::each_chunk(&incoming_tombstones, |chunk, _| -> Result<()> {
        let sql = format!(
            "INSERT OR REPLACE INTO temp.{table_name} (
                    guid
                ) VALUES {vals}",
            table_name = table_name,
            vals = sql_support::repeat_sql_values(chunk.len())
        );
        signal.err_if_interrupted()?;

        let params: Vec<&dyn ToSql> = chunk.iter().map(|r| r.0.id() as &dyn ToSql).collect();
        conn.execute(&sql, &params)?;
        Ok(())
    })
}

/// Incoming tombstones are retrieved from the staging table
/// and assigned `IncomingState` values.
/// This function makes a number of implied assumptions about the sql.
pub(super) fn common_get_incoming_tombstone_states<T: SyncRecord>(
    conn: &Connection,
    sql: &str,
) -> Result<Vec<IncomingState<T>>> {
    Ok(conn
        .conn()
        .query_rows_and_then_named(sql, &[], |row| -> Result<IncomingState<T>> {
            let incoming_guid: Guid = row.get_unwrap("s_guid");
            let have_local_record = row.get::<_, Option<Guid>>("l_guid")?.is_some();
            let have_local_tombstone = row.get::<_, Option<Guid>>("t_guid")?.is_some();

            let local = if have_local_record {
                let record = T::from_row(row, "")?;
                let meta = record.metadata();
                let has_local_changes = meta.sync_change_counter.unwrap_or(0) != 0;
                if has_local_changes {
                    LocalRecordInfo::Modified { record }
                } else {
                    LocalRecordInfo::Unmodified { record }
                }
            } else if have_local_tombstone {
                LocalRecordInfo::Tombstone {
                    guid: incoming_guid.clone(),
                }
            } else {
                LocalRecordInfo::Missing
            };
            Ok(IncomingState {
                incoming: IncomingRecordInfo::Tombstone {
                    guid: incoming_guid,
                },
                local,
                mirror: None, // XXX - we *might* have a mirror record, but don't really care.
            })
        })?)
}

/// Incoming records (excluding tombstones) are retrieved from the staging table
/// and assigned `IncomingState` values.
pub(super) fn common_get_incoming_record_states<T: SyncRecord>(
    conn: &Connection,
    sql: &str,
) -> Result<Vec<IncomingState<T>>> {
    Ok(conn
        .conn()
        .query_rows_and_then_named(sql, &[], |row| -> Result<IncomingState<T>> {
            let mirror_exists: bool = row.get::<_, Guid>("m_guid").is_ok();
            let local_exists: bool = row.get::<_, Guid>("l_guid").is_ok();
            let tombstone_guid: Option<Guid> = row.get_unwrap("t_guid");

            let incoming = T::from_row(row, "s_")?;
            let mirror = if mirror_exists {
                Some(T::from_row(row, "m_")?)
            } else {
                None
            };
            let local = if local_exists {
                let record = T::from_row(row, "l_")?;
                let has_changes = record.metadata().sync_change_counter.unwrap_or(0) != 0;
                if has_changes {
                    LocalRecordInfo::Modified { record }
                } else {
                    LocalRecordInfo::Unmodified { record }
                }
            } else {
                match tombstone_guid {
                    None => LocalRecordInfo::Missing,
                    Some(guid) => LocalRecordInfo::Tombstone { guid },
                }
            };

            Ok(IncomingState {
                incoming: IncomingRecordInfo::Record { record: incoming },
                local,
                mirror,
            })
        })?)
}

pub(super) fn common_remove_record(conn: &Connection, table_name: &str, guid: &Guid) -> Result<()> {
    conn.execute_named(
        &format!(
            "DELETE FROM {}
            WHERE guid = :guid",
            table_name
        ),
        rusqlite::named_params! {
            ":guid": guid,
        },
    )?;
    Ok(())
}

pub(super) fn common_change_guid(
    conn: &Connection,
    table_name: &str,
    old_guid: &Guid,
    new_guid: &Guid,
) -> Result<()> {
    assert_ne!(old_guid, new_guid);
    let nrows = conn.execute_named(
        &format!(
            "UPDATE {}
            SET guid = :new_guid,
            sync_change_counter = sync_change_counter + 1
            WHERE guid = :old_guid",
            table_name
        ),
        rusqlite::named_params! {
            ":old_guid": old_guid,
            ":new_guid": new_guid,
        },
    )?;
    // something's gone badly wrong if this didn't affect exactly 1 row.
    assert_eq!(nrows, 1);
    Ok(())
}

// A macro for our record merge implementation.
// We allow all "common" fields from the sub-types to be getters on the
// InsertableItem type.
// Macros don't have fine-grained visibility and is visible to the entire
// crate, so we give it a very specific name.
#[macro_export]
macro_rules! sync_merge_field_check {
    ($field_name:ident,
    $incoming:ident,
    $local:ident,
    $mirror:ident,
    $merged_record:ident
    ) => {
        let incoming_field = &$incoming.$field_name;
        let local_field = &$local.$field_name;
        let is_local_same;
        let is_incoming_same;

        match &$mirror {
            Some(m) => {
                let mirror_field = &m.$field_name;
                is_local_same = mirror_field == local_field;
                is_incoming_same = mirror_field == incoming_field;
            }
            None => {
                is_local_same = true;
                is_incoming_same = local_field == incoming_field;
            }
        };

        let should_use_local = is_incoming_same || local_field == incoming_field;

        if is_local_same && !is_incoming_same {
            $merged_record.$field_name = incoming_field.clone();
        } else if should_use_local {
            $merged_record.$field_name = local_field.clone();
        } else {
            // There are conflicting differences, so we "fork" the record - we
            // will end up giving the local one a new guid and save the remote
            // one with its incoming ID.
            return MergeResult::Forked {
                forked: get_forked_record($local.clone()),
            };
        }
    };
}

// And common helpers for tests (although no actual tests!)
#[cfg(test)]
pub(super) mod tests {
    use super::super::*;
    use interrupt_support::NeverInterrupts;
    use serde_json::{json, Value};
    use sync15::ServerTimestamp;

    pub(in crate::sync) fn array_to_incoming<T: for<'de> serde::Deserialize<'de>>(
        vals: Vec<Value>,
    ) -> IncomingRecords<T> {
        let mut records = Vec::with_capacity(vals.len());
        let mut tombstones = Vec::with_capacity(vals.len());
        for elt in vals {
            if elt.get("deleted").is_some() {
                tombstones.push((
                    serde_json::from_value::<T>(elt.clone()).expect("must be valid"),
                    ServerTimestamp::from_millis(0),
                ))
            } else {
                records.push((
                    serde_json::from_value::<T>(elt.clone()).expect("must be valid"),
                    ServerTimestamp::from_millis(0),
                ));
            }
        }
        IncomingRecords {
            records,
            tombstones,
        }
    }

    pub(in crate::sync) fn expand_test_guid(c: char) -> String {
        c.to_string().repeat(12)
    }

    pub(in crate::sync) fn test_json_tombstone(guid_prefix: char) -> Value {
        let t = json! {
            {
                "id": expand_test_guid(guid_prefix),
                "deleted": true,
            }
        };
        t
    }

    pub(in crate::sync) fn do_test_incoming_same<T: SyncRecord + std::fmt::Debug + Clone>(
        ri: &dyn RecordStorageImpl<Record = T>,
        record: T,
    ) {
        ri.insert_local_record(record.clone())
            .expect("insert should work");
        let incoming = IncomingRecords {
            records: vec![(record, ServerTimestamp::from_millis(0))],
            tombstones: vec![],
        };
        ri.stage_incoming(incoming, &NeverInterrupts)
            .expect("stage should work");
        let mut states = ri.fetch_incoming_states().expect("fetch should work");
        assert_eq!(states.len(), 1, "1 records == 1 state!");
        let action =
            crate::sync::plan_incoming(ri, states.pop().unwrap()).expect("plan should work");
        // Even though the records are identical, we still merged the metadata
        // so treat this as an Update.
        assert!(matches!(action, crate::sync::IncomingAction::Update { .. }));
    }
}

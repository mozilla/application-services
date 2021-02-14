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
use crate::sync::{IncomingRecord, IncomingState, LocalRecordInfo, Payload, SyncRecord};
use interrupt_support::Interruptee;
use rusqlite::{types::ToSql, Connection};
use serde::Deserialize;
use sql_support::ConnExt;
use sync15::ServerTimestamp;
use sync_guid::Guid;

/// Stages incoming records (excluding incoming tombstones) in preparation for
/// applying incoming changes for the syncing autofill records.
pub(super) fn common_stage_incoming_records(
    conn: &Connection,
    table_name: &str,
    incoming: Vec<(Payload, ServerTimestamp)>,
    signal: &dyn Interruptee,
) -> Result<()> {
    log::info!(
        "staging {} incoming records into {}",
        incoming.len(),
        table_name
    );
    let chunk_size = 2;
    let vals: Vec<(String, String)> = incoming
        .into_iter()
        .map(|(p, _)| (p.id().to_string(), p.into_json_string()))
        .collect();
    sql_support::each_sized_chunk(
        &vals,
        sql_support::default_max_variable_number() / chunk_size,
        |chunk, _| -> Result<()> {
            signal.err_if_interrupted()?;
            let sql = format!(
                "INSERT OR REPLACE INTO temp.{table_name} (guid, payload)
                 VALUES {vals}",
                table_name = table_name,
                vals = sql_support::repeat_multi_values(chunk.len(), 2)
            );
            let mut params = Vec::with_capacity(chunk.len() * chunk_size);
            for (guid, json) in chunk {
                params.push(guid as &dyn ToSql);
                params.push(json);
            }
            conn.execute(&sql, params)?;
            Ok(())
        },
    )?;
    log::trace!("staged");
    Ok(())
}

/// Incoming records are retrieved from the staging and mirror tables and assigned `IncomingState` values.
pub(super) fn common_fetch_incoming_record_states<T: SyncRecord + for<'a> Deserialize<'a>>(
    conn: &Connection,
    sql: &str,
) -> Result<Vec<IncomingState<T>>> {
    Ok(conn
        .conn()
        .query_rows_and_then_named(sql, &[], |row| -> Result<IncomingState<T>> {
            // the 'guid' and 's_payload' rows must be non-null.
            let guid: Guid = row.get_unwrap("guid");
            // all other rows may be null.
            let tombstone_exists = row.get_unwrap::<_, Option<String>>("t_guid").is_some();
            let local_exists = row.get_unwrap::<_, Option<String>>("l_guid").is_some();
            let mirror_payload: Option<String> = row.get_unwrap("m_payload");

            let staged_payload: String = row.get_unwrap("s_payload");
            let payload = Payload::from_json(serde_json::from_str(&staged_payload)?)?;
            let incoming = if payload.is_tombstone() {
                IncomingRecord::Tombstone {
                    guid: payload.id().into(),
                }
            } else {
                IncomingRecord::Record {
                    record: payload.into_record()?,
                }
            };
            let mirror = match mirror_payload {
                Some(payload) => serde_json::from_str(&payload)?,
                None => None,
            };
            let local = if local_exists {
                let record = T::from_row(row)?;
                let has_changes = record.metadata().sync_change_counter.unwrap_or(0) != 0;
                if has_changes {
                    LocalRecordInfo::Modified { record }
                } else {
                    LocalRecordInfo::Unmodified { record }
                }
            } else if tombstone_exists {
                LocalRecordInfo::Tombstone { guid }
            } else {
                LocalRecordInfo::Missing
            };

            Ok(IncomingState {
                incoming,
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

    pub(in crate::sync) fn array_to_incoming(vals: Vec<Value>) -> Vec<(Payload, ServerTimestamp)> {
        vals.into_iter()
            .map(|v| {
                (
                    Payload::from_json(v).expect("should be a payload"),
                    ServerTimestamp::from_millis(0),
                )
            })
            .collect()
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

    // Incoming record is identical to a local record.
    pub(in crate::sync) fn do_test_incoming_same<
        T: SyncRecord + std::fmt::Debug + Clone + Serialize,
    >(
        ri: &dyn RecordStorageImpl<Record = T>,
        record: T,
    ) {
        ri.insert_local_record(record.clone())
            .expect("insert should work");
        let payload = Payload::from_record(record).expect("should serialize");
        ri.stage_incoming(
            vec![(payload, ServerTimestamp::from_millis(0))],
            &NeverInterrupts,
        )
        .expect("stage should work");
        let mut states = ri.fetch_incoming_states().expect("fetch should work");
        assert_eq!(states.len(), 1, "1 records == 1 state!");
        let action =
            crate::sync::plan_incoming(ri, states.pop().unwrap()).expect("plan should work");
        // Even though the records are identical, we still merged the metadata
        // so treat this as an Update.
        assert!(matches!(action, crate::sync::IncomingAction::Update { .. }));
    }

    // Incoming tombstone for an existing local record.
    pub(in crate::sync) fn do_test_incoming_tombstone<
        T: SyncRecord + std::fmt::Debug + Clone + Serialize,
    >(
        ri: &dyn RecordStorageImpl<Record = T>,
        record: T,
    ) {
        let guid = record.id().clone();
        ri.insert_local_record(record).expect("insert should work");
        let payload = Payload::new_tombstone(guid);
        ri.stage_incoming(
            vec![(payload, ServerTimestamp::from_millis(0))],
            &NeverInterrupts,
        )
        .expect("stage should work");
        let mut states = ri.fetch_incoming_states().expect("fetch should work");
        assert_eq!(states.len(), 1, "1 records == 1 state!");
        let action =
            crate::sync::plan_incoming(ri, states.pop().unwrap()).expect("plan should work");
        // Even though the records are identical, we still merged the metadata
        // so treat this as an Update.
        assert!(matches!(action, crate::sync::IncomingAction::DeleteLocalRecord { .. }));
    }
}

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
use error_support::{error, info, trace};
use interrupt_support::Interruptee;
use rusqlite::{types::ToSql, Connection, Row};
use sync15::bso::OutgoingBso;
use sync15::ServerTimestamp;
use sync_guid::Guid;

/// Stages incoming records (excluding incoming tombstones) in preparation for
/// applying incoming changes for the syncing autofill records.
/// The incoming record is a String, because for credit-cards it is the encrypted
/// version of a JSON record.
pub(super) fn common_stage_incoming_records(
    conn: &Connection,
    table_name: &str,
    incoming: Vec<(Guid, String, ServerTimestamp)>,
    signal: &dyn Interruptee,
) -> Result<()> {
    info!(
        "staging {} incoming records into {}",
        incoming.len(),
        table_name
    );
    let chunk_size = 2;
    let vals: Vec<(Guid, String)> = incoming
        .into_iter()
        .map(|(guid, data, _)| (guid, data))
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
            conn.execute(&sql, rusqlite::params_from_iter(params))?;
            Ok(())
        },
    )?;
    trace!("staged");
    Ok(())
}

pub(super) fn common_remove_record(conn: &Connection, table_name: &str, guid: &Guid) -> Result<()> {
    conn.execute(
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

// This optionally takes a mirror table name to update if
// there is a corresponding record with the same guid we'd like to update
pub(super) fn common_change_guid(
    conn: &Connection,
    table_name: &str,
    mirror_table_name: &str,
    old_guid: &Guid,
    new_guid: &Guid,
) -> Result<()> {
    assert_ne!(old_guid, new_guid);
    let nrows = conn.execute(
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

    // If there is also a corresponding mirror row (e.g forking situations), update aswell
    conn.execute(
        &format!(
            "UPDATE {}
                SET guid = :new_guid
                WHERE guid = :old_guid",
            mirror_table_name
        ),
        rusqlite::named_params! {
            ":old_guid": old_guid,
            ":new_guid": new_guid,
        },
    )?;

    Ok(())
}

/// Records in the incoming staging table need to end up in the mirror.
pub(super) fn common_mirror_staged_records(
    conn: &Connection,
    staging_table_name: &str,
    mirror_table_name: &str,
) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO {} (guid, payload)
             SELECT guid, payload FROM temp.{}",
            mirror_table_name, staging_table_name,
        ),
        [],
    )?;
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

pub(super) fn common_get_outgoing_staging_records(
    conn: &Connection,
    data_sql: &str,
    tombstones_sql: &str,
    payload_from_data_row: &dyn Fn(&Row<'_>) -> Result<(OutgoingBso, i64)>,
) -> anyhow::Result<Vec<(OutgoingBso, i64)>> {
    let outgoing_records =
        common_get_outgoing_records(conn, data_sql, tombstones_sql, payload_from_data_row)?;
    Ok(outgoing_records.into_iter().collect::<Vec<_>>())
}

fn get_outgoing_records(
    conn: &Connection,
    sql: &str,
    record_from_data_row: &dyn Fn(&Row<'_>) -> Result<(OutgoingBso, i64)>,
) -> anyhow::Result<Vec<(OutgoingBso, i64)>> {
    conn.prepare(sql)?
        .query_map([], |row| Ok(record_from_data_row(row)))?
        .map(|r| {
            r.unwrap().map_err(|e| {
                error!(
                    "Failed to retrieve a record from a row with the following error: {}",
                    e
                );
                e.into()
            })
        })
        .collect::<std::result::Result<Vec<_>, _>>()
}

pub(super) fn common_get_outgoing_records(
    conn: &Connection,
    data_sql: &str,
    tombstone_sql: &str,
    record_from_data_row: &dyn Fn(&Row<'_>) -> Result<(OutgoingBso, i64)>,
) -> anyhow::Result<Vec<(OutgoingBso, i64)>> {
    let mut payload = get_outgoing_records(conn, data_sql, record_from_data_row)?;

    payload.append(&mut get_outgoing_records(conn, tombstone_sql, &|row| {
        Ok((
            OutgoingBso::new_tombstone(Guid::from_string(row.get("guid")?).into()),
            0,
        ))
    })?);

    Ok(payload)
}

pub(super) fn common_save_outgoing_records(
    conn: &Connection,
    table_name: &str,
    staging_records: Vec<(Guid, String, i64)>,
) -> anyhow::Result<()> {
    let chunk_size = 3;
    sql_support::each_sized_chunk(
        &staging_records,
        sql_support::default_max_variable_number() / chunk_size,
        |chunk, _| -> anyhow::Result<()> {
            let sql = format!(
                "INSERT OR REPLACE INTO temp.{table_name} (guid, payload, sync_change_counter)
                VALUES {staging_records}",
                table_name = table_name,
                staging_records = sql_support::repeat_multi_values(chunk.len(), chunk_size)
            );
            let mut params = Vec::with_capacity(chunk.len() * chunk_size);
            for (guid, json, sync_change_counter) in chunk {
                params.push(guid as &dyn ToSql);
                params.push(json);
                params.push(sync_change_counter);
            }
            conn.execute(&sql, rusqlite::params_from_iter(params))?;
            Ok(())
        },
    )?;
    Ok(())
}

pub(super) fn common_finish_synced_items(
    conn: &Connection,
    data_table_name: &str,
    mirror_table_name: &str,
    outgoing_staging_table_name: &str,
    records_synced: Vec<Guid>,
) -> anyhow::Result<()> {
    // Update the local change counter for uploaded items.
    reset_sync_change_counter(
        conn,
        data_table_name,
        outgoing_staging_table_name,
        records_synced,
    )?;
    // Copy from the outgoing staging table into the mirror.
    let sql = format!(
        "INSERT OR REPLACE INTO {mirror_table_name}
            SELECT guid, payload FROM temp.{outgoing_staging_table_name}",
        mirror_table_name = mirror_table_name,
        outgoing_staging_table_name = outgoing_staging_table_name,
    );
    conn.execute(&sql, [])?;
    Ok(())
}

// When we started syncing, we saved `sync_change_counter` in the staging
// table for every record. Now that we've uploaded the server, we need to
// decrement that value from the current value - anything that ends up with
// a non-zero value must have changed since while we were uploading so remains
// dirty. This does that decrement.
fn reset_sync_change_counter(
    conn: &Connection,
    data_table_name: &str,
    outgoing_table_name: &str,
    records_synced: Vec<Guid>,
) -> anyhow::Result<()> {
    sql_support::each_chunk(&records_synced, |chunk, _| -> anyhow::Result<()> {
        conn.execute(
            &format!(
                // We're making two checks that in practice should be redundant. First we're limiting the
                // number of records that we're pulling from the outgoing staging table to one. Lastly we're
                // ensuring that the updated local records are also in `records_synced` which should be the
                // case since the sync will fail entirely if the server rejects individual records.
                "UPDATE {data_table_name} AS data
                SET sync_change_counter = sync_change_counter -
                    (
                        SELECT outgoing.sync_change_counter
                        FROM temp.{outgoing_table_name} AS outgoing
                        WHERE outgoing.guid = data.guid LIMIT 1
                    )
                WHERE guid IN ({values})",
                data_table_name = data_table_name,
                outgoing_table_name = outgoing_table_name,
                values = sql_support::repeat_sql_values(chunk.len())
            ),
            rusqlite::params_from_iter(chunk),
        )?;
        Ok(())
    })?;

    Ok(())
}

// And common helpers for tests (although no actual tests!)
#[cfg(test)]
pub(super) mod tests {
    use super::super::*;
    use interrupt_support::NeverInterrupts;
    use serde_json::{json, Value};

    pub(in crate::sync) fn array_to_incoming(vals: Vec<Value>) -> Vec<IncomingBso> {
        vals.into_iter()
            .map(IncomingBso::from_test_content)
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
    pub(in crate::sync) fn do_test_incoming_same<T: SyncRecord + std::fmt::Debug + Clone>(
        ri: &dyn ProcessIncomingRecordImpl<Record = T>,
        tx: &Transaction<'_>,
        record: T,
        bso: IncomingBso,
    ) {
        ri.insert_local_record(tx, record)
            .expect("insert should work");
        ri.stage_incoming(tx, vec![bso], &NeverInterrupts)
            .expect("stage should work");
        let mut states = ri.fetch_incoming_states(tx).expect("fetch should work");
        assert_eq!(states.len(), 1, "1 records == 1 state!");
        let action =
            crate::sync::plan_incoming(ri, tx, states.pop().unwrap()).expect("plan should work");
        // Even though the records are identical, we still merged the metadata
        // so treat this as an Update.
        assert!(matches!(action, crate::sync::IncomingAction::Update { .. }));
    }

    // Incoming tombstone for an existing local record.
    pub(in crate::sync) fn do_test_incoming_tombstone<T: SyncRecord + std::fmt::Debug + Clone>(
        ri: &dyn ProcessIncomingRecordImpl<Record = T>,
        tx: &Transaction<'_>,
        record: T,
    ) {
        let guid = record.id().clone();
        ri.insert_local_record(tx, record)
            .expect("insert should work");
        ri.stage_incoming(
            tx,
            vec![IncomingBso::new_test_tombstone(guid)],
            &NeverInterrupts,
        )
        .expect("stage should work");
        let mut states = ri.fetch_incoming_states(tx).expect("fetch should work");
        assert_eq!(states.len(), 1, "1 records == 1 state!");
        let action =
            crate::sync::plan_incoming(ri, tx, states.pop().unwrap()).expect("plan should work");
        // Even though the records are identical, we still merged the metadata
        // so treat this as an Update.
        assert!(matches!(
            action,
            crate::sync::IncomingAction::DeleteLocalRecord { .. }
        ));
    }

    // local record was scrubbed of encrypted data -- we should update it using server data
    pub(in crate::sync) fn do_test_scrubbed_local_data<T: SyncRecord + std::fmt::Debug + Clone>(
        ri: &dyn ProcessIncomingRecordImpl<Record = T>,
        tx: &Transaction<'_>,
        record: T,
        bso: IncomingBso,
    ) {
        ri.insert_local_record(tx, record)
            .expect("insert should work");
        ri.stage_incoming(tx, vec![bso], &NeverInterrupts)
            .expect("stage should work");
        let mut states = ri.fetch_incoming_states(tx).expect("fetch should work");
        assert_eq!(states.len(), 1, "1 records == 1 state!");
        assert!(
            matches!(states[0].local, LocalRecordInfo::Scrubbed { .. }),
            "state should be LocalRecordInfo::Scubbed but it is: {:?}",
            states[0].local
        );

        let action =
            crate::sync::plan_incoming(ri, tx, states.pop().unwrap()).expect("plan should work");
        assert!(matches!(action, crate::sync::IncomingAction::Update { .. }));
    }

    // "Staged" records are moved to the mirror by finish_incoming().
    pub(in crate::sync) fn do_test_staged_to_mirror<T: SyncRecord + std::fmt::Debug + Clone>(
        ri: &dyn ProcessIncomingRecordImpl<Record = T>,
        tx: &Transaction<'_>,
        record: T,
        bso1: IncomingBso,
        mirror_table_name: &str,
    ) {
        let guid1 = record.id().clone();
        let guid2 = Guid::random();
        let bso2 = IncomingBso::new_test_tombstone(guid2.clone());

        ri.stage_incoming(tx, vec![bso1, bso2], &NeverInterrupts)
            .expect("stage should work");

        ri.finish_incoming(tx).expect("finish should work");

        let sql = format!(
            "SELECT COUNT(*) FROM {} where guid = '{}' OR guid = '{}'",
            mirror_table_name, guid1, guid2
        );
        let num_rows = tx
            .query_row(&sql, [], |row| Ok(row.get::<_, u32>(0).unwrap()))
            .unwrap();
        assert_eq!(num_rows, 2);
    }

    fn exists_in_table(tx: &Transaction<'_>, table_name: &str, guid: &Guid) {
        let sql = format!(
            "SELECT COUNT(*) FROM {} where guid = '{}'",
            table_name, guid
        );
        let num_rows = tx
            .query_row(&sql, [], |row| Ok(row.get::<_, u32>(0).unwrap()))
            .unwrap();
        assert_eq!(num_rows, 1);
    }

    pub(in crate::sync) fn exists_with_counter_value_in_table(
        tx: &Transaction<'_>,
        table_name: &str,
        guid: &Guid,
        expected_counter_value: i64,
    ) {
        let sql = format!(
            "SELECT COUNT(*)
            FROM {table_name}
            WHERE sync_change_counter = {expected_counter_value}
                AND guid = :guid",
            table_name = table_name,
            expected_counter_value = expected_counter_value,
        );

        let num_rows = tx
            .query_row(&sql, [guid], |row| Ok(row.get::<_, u32>(0).unwrap()))
            .unwrap();
        assert_eq!(num_rows, 1);
    }

    pub(in crate::sync) fn do_test_outgoing_never_synced<
        T: SyncRecord + std::fmt::Debug + Clone,
    >(
        tx: &Transaction<'_>,
        ro: &dyn ProcessOutgoingRecordImpl<Record = T>,
        guid: &Guid,
        data_table_name: &str,
        mirror_table_name: &str,
        staging_table_name: &str,
    ) {
        // call fetch outgoing records
        assert!(ro.fetch_outgoing_records(tx).is_ok());

        // check that the record is in the outgoing table
        exists_in_table(tx, &format!("temp.{}", staging_table_name), guid);

        // call push synced items
        assert!(ro.finish_synced_items(tx, vec![guid.clone()]).is_ok());

        // check that the sync change counter
        exists_with_counter_value_in_table(tx, data_table_name, guid, 0);

        // check that the outgoing record is in the mirror
        exists_in_table(tx, mirror_table_name, guid);
    }

    pub(in crate::sync) fn do_test_outgoing_tombstone<T: SyncRecord + std::fmt::Debug + Clone>(
        tx: &Transaction<'_>,
        ro: &dyn ProcessOutgoingRecordImpl<Record = T>,
        guid: &Guid,
        data_table_name: &str,
        mirror_table_name: &str,
        staging_table_name: &str,
    ) {
        // call fetch outgoing records
        assert!(ro.fetch_outgoing_records(tx).is_ok());

        // check that the record is in the outgoing table
        exists_in_table(tx, &format!("temp.{}", staging_table_name), guid);

        // call push synced items
        assert!(ro.finish_synced_items(tx, vec![guid.clone()]).is_ok());

        // check that the record wasn't copied to the data table
        let sql = format!(
            "SELECT COUNT(*) FROM {} where guid = '{}'",
            data_table_name, guid
        );
        let num_rows = tx
            .query_row(&sql, [], |row| Ok(row.get::<_, u32>(0).unwrap()))
            .unwrap();
        assert_eq!(num_rows, 0);

        // check that the outgoing record is in the mirror
        exists_in_table(tx, mirror_table_name, guid);
    }

    pub(in crate::sync) fn do_test_outgoing_synced_with_local_change<
        T: SyncRecord + std::fmt::Debug + Clone,
    >(
        tx: &Transaction<'_>,
        ro: &dyn ProcessOutgoingRecordImpl<Record = T>,
        guid: &Guid,
        data_table_name: &str,
        mirror_table_name: &str,
        staging_table_name: &str,
    ) {
        // call fetch outgoing records
        assert!(ro.fetch_outgoing_records(tx).is_ok());

        // check that the record is in the outgoing table
        exists_in_table(tx, &format!("temp.{}", staging_table_name), guid);

        // call push synced items
        assert!(ro.finish_synced_items(tx, vec![guid.clone()]).is_ok());

        // check that the sync change counter
        exists_with_counter_value_in_table(tx, data_table_name, guid, 0);

        // check that the outgoing record is in the mirror
        exists_in_table(tx, mirror_table_name, guid);
    }

    pub(in crate::sync) fn do_test_outgoing_synced_with_no_change<
        T: SyncRecord + std::fmt::Debug + Clone,
    >(
        tx: &Transaction<'_>,
        ro: &dyn ProcessOutgoingRecordImpl<Record = T>,
        guid: &Guid,
        data_table_name: &str,
        staging_table_name: &str,
    ) {
        // call fetch outgoing records
        assert!(ro.fetch_outgoing_records(tx).is_ok());

        // check that the record is not in the outgoing table
        let sql = format!(
            "SELECT COUNT(*) FROM {} where guid = '{}'",
            &format!("temp.{}", staging_table_name),
            guid
        );
        let num_rows = tx
            .query_row(&sql, [], |row| Ok(row.get::<_, u32>(0).unwrap()))
            .unwrap();
        assert_eq!(num_rows, 0);

        // call push synced items
        assert!(ro.finish_synced_items(tx, Vec::<Guid>::new()).is_ok());

        // check that the sync change counter is unchanged
        exists_with_counter_value_in_table(tx, data_table_name, guid, 0);
    }
}

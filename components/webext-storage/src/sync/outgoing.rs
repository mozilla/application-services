/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// The "outgoing" part of syncing - building the payloads to upload and
// managing the sync state of the local DB.

use interrupt_support::Interruptee;
use rusqlite::{Row, Transaction};
use sql_support::ConnExt;
use sync15_traits::ServerTimestamp;
use sync_guid::Guid as SyncGuid;

use crate::error::*;

use super::ServerPayload;

fn outgoing_from_row(row: &Row<'_>) -> Result<(ServerPayload, i32)> {
    let mirror_guid = row.get::<_, Option<SyncGuid>>("mirror_guid")?;
    let staging_guid = row.get::<_, Option<SyncGuid>>("staging_guid")?;
    // We could assert they are identical if non-none?
    let guid = mirror_guid
        .or(staging_guid)
        .unwrap_or_else(SyncGuid::random);
    let ext_id: String = row.get("ext_id")?;
    let raw_data: Option<String> = row.get("data")?;
    let (data, deleted) = if raw_data.is_some() {
        (raw_data, false)
    } else {
        (None, true)
    };
    let change_counter = row.get::<_, i32>("sync_change_counter")?;
    Ok((
        ServerPayload {
            ext_id,
            guid,
            data,
            deleted,
            last_modified: ServerTimestamp(0),
        },
        change_counter,
    ))
}

/// Gets info about what should be uploaded and also records metadata about these
/// items in a temp table. Returns a vec of the payloads which
/// should be uploaded. record_uploaded() can be called after the upload is
/// complete and the data in the temp table will be used to update the local
/// store.
#[allow(dead_code)] // Kill this annotation once the bridged engine is hooked up
pub fn get_and_record_outgoing(
    tx: &Transaction<'_>,
    _signal: &dyn Interruptee,
) -> Result<Vec<ServerPayload>> {
    // The item may not yet have a GUID (ie, it might not already be in either
    // the mirror nor the incoming staging table.) We could probably perform
    // some impressive sql gymnastics to handle this and arrange for the
    // populating of the outgoing staging table to be done via a single
    // `INSERT INTO ... SELECT` statement - but the fact we need to extract each
    // record from this query anyway means we just loop in rust.
    let sql = "SELECT l.ext_id, l.data, l.sync_change_counter,
               m.guid as mirror_guid, s.guid as staging_guid
               FROM storage_sync_data l
               -- left joins as one or both may not exist.
               LEFT JOIN storage_sync_mirror m ON m.ext_id = l.ext_id
               LEFT JOIN storage_sync_staging s ON s.ext_id = l.ext_id
               WHERE sync_change_counter > 0";
    let elts = tx
        .conn()
        .query_rows_and_then_named(sql, &[], outgoing_from_row)?;

    log::debug!("get_outgoing found {} items", elts.len());
    // Now the temp table thang...
    let tt_sql = "INSERT INTO storage_sync_outgoing_staging
                  (guid, ext_id, data, sync_change_counter)
                  VALUES (:guid, :ext_id, :data, :sync_change_counter)";
    for (payload, change_counter) in &elts {
        log::trace!("outgoing '{:?}' with counter={}", payload, change_counter);
        tx.execute_named_cached(
            tt_sql,
            rusqlite::named_params! {
                ":guid": payload.guid,
                ":ext_id": payload.ext_id,
                ":data": payload.data,
                ":sync_change_counter": change_counter
            },
        )?;
    }
    Ok(elts.into_iter().map(|e| e.0).collect())
}

/// Record the fact that items were uploaded. This updates the state of the
/// local DB to reflect the state of the server we just updated.
/// Note that this call is almost certainly going to be made in a *different*
/// transaction than the transaction used in `get_and_record_outgoing()`
#[allow(dead_code)] // Kill this annotation once the bridged engine is hooked up
pub fn record_uploaded(
    tx: &Transaction<'_>,
    items: &[SyncGuid],
    signal: &dyn Interruptee,
) -> Result<()> {
    log::debug!(
        "record_uploaded recording that {} items were uploaded",
        items.len()
    );

    let sql = "
        UPDATE storage_sync_data SET
            sync_change_counter =
                (sync_change_counter - (
                    SELECT sync_change_counter
                    FROM storage_sync_outgoing_staging
                    WHERE storage_sync_outgoing_staging.guid = :guid)
                )
        WHERE ext_id = (SELECT ext_id
                        FROM storage_sync_outgoing_staging
                        WHERE storage_sync_outgoing_staging.guid = :guid)";
    for guid in items.iter() {
        signal.err_if_interrupted()?;
        log::trace!("recording guid='{}' was uploaded", guid);
        tx.execute_named(
            sql,
            rusqlite::named_params! {
                ":guid": guid,
            },
        )?;
    }

    // Copy incoming staging into the mirror, then outgoing staging to the
    // mirror and local tombstones.
    tx.execute_batch(
        "
        INSERT OR REPLACE INTO storage_sync_mirror (guid, ext_id, data)
        SELECT guid, ext_id, data FROM temp.storage_sync_staging;

        INSERT OR REPLACE INTO storage_sync_mirror (guid, ext_id, data)
        SELECT guid, ext_id, data FROM temp.storage_sync_outgoing_staging;

        DELETE FROM storage_sync_data WHERE data IS NULL AND sync_change_counter = 0;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test::new_syncable_mem_db;
    use super::*;
    use interrupt_support::NeverInterrupts;

    #[test]
    fn test_simple() -> Result<()> {
        let mut db = new_syncable_mem_db();
        let tx = db.transaction()?;

        tx.execute_batch(
            r#"
            INSERT INTO storage_sync_data (ext_id, data, sync_change_counter)
            VALUES
                ('ext_no_changes', '{"foo":"bar"}', 0),
                ('ext_with_changes', '{"foo":"bar"}', 1);
        "#,
        )?;

        let changes = get_and_record_outgoing(&tx, &NeverInterrupts)?;
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].ext_id, "ext_with_changes".to_string());

        record_uploaded(
            &tx,
            changes
                .into_iter()
                .map(|p| p.guid)
                .collect::<Vec<SyncGuid>>()
                .as_slice(),
            &NeverInterrupts,
        )?;

        let counter: i32 = tx.conn().query_one(
            "SELECT sync_change_counter FROM storage_sync_data WHERE ext_id = 'ext_with_changes'",
        )?;
        assert_eq!(counter, 0);
        Ok(())
    }
}

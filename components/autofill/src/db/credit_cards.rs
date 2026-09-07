/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::{
    models::{
        credit_card::{
            CreditCardMeta, InternalCreditCard, UpdatableCreditCardFields,
            UpdatableCreditCardFieldsWithMeta,
        },
        Metadata,
    },
    schema::{CREDIT_CARD_COMMON_COLS, CREDIT_CARD_COMMON_VALS},
    timestamp_from_millis, with_savepoint, CounterUpdate,
};
use crate::error::*;

use jwcrypto::EncryptorDecryptor;
use rusqlite::{Connection, Transaction};
use sync_guid::Guid;
use types::Timestamp;

pub struct CreditCardsDeletionMetrics {
    pub total_scrubbed_records: u64,
}

pub(crate) fn add_credit_card(
    conn: &Connection,
    new_credit_card_fields: UpdatableCreditCardFields,
) -> Result<InternalCreditCard> {
    let now = Timestamp::now();

    // We return an InternalCreditCard, so set it up first, including the
    // missing fields, before we insert it.
    let credit_card = InternalCreditCard {
        guid: Guid::random(),
        cc_name: new_credit_card_fields.cc_name,
        cc_number_enc: new_credit_card_fields.cc_number_enc,
        cc_number_last_4: new_credit_card_fields.cc_number_last_4,
        cc_exp_month: new_credit_card_fields.cc_exp_month,
        cc_exp_year: new_credit_card_fields.cc_exp_year,
        // Credit card types are a fixed set of strings as defined in the link below
        // (https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
        cc_type: new_credit_card_fields.cc_type,
        metadata: Metadata {
            time_created: now,
            time_last_modified: now,
            ..Default::default()
        },
    };

    let tx = conn.unchecked_transaction()?;
    add_internal_credit_card(&tx, &credit_card)?;
    tx.commit()?;
    Ok(credit_card)
}

/// Adds a credit card **including metadata**, taking the guid, timestamps and
/// sync change counter from the caller rather than generating them. Normally you
/// will use `add_credit_card` instead; this is for importing records from
/// another store that already have metadata.
///
/// `cc_number_enc` is stored exactly as given and is not checked against the
/// store's key, matching `add_credit_card`. An importing application owns the
/// ciphertext it supplies.
pub(crate) fn add_credit_card_with_meta(
    conn: &Connection,
    fields: UpdatableCreditCardFields,
    meta: CreditCardMeta,
) -> Result<InternalCreditCard> {
    let tx = conn.unchecked_transaction()?;
    let card = internal_credit_card_from_meta(fields, &meta);
    add_internal_credit_card(&tx, &card)?;
    tx.commit()?;
    Ok(card)
}

/// Adds multiple credit cards **including metadata** within a single
/// transaction. Each record gets its own result, so a record that fails to
/// insert is reported as `Err(message)` without aborting the rest of the batch.
pub(crate) fn add_many_credit_cards_with_meta(
    conn: &Connection,
    entries: Vec<UpdatableCreditCardFieldsWithMeta>,
) -> Result<Vec<std::result::Result<InternalCreditCard, String>>> {
    let tx = conn.unchecked_transaction()?;
    let mut results = Vec::with_capacity(entries.len());
    for entry in entries {
        let card = internal_credit_card_from_meta(entry.fields, &entry.meta);
        match with_savepoint(&tx, || add_internal_credit_card(&tx, &card))? {
            Ok(()) => results.push(Ok(card)),
            Err(e) => results.push(Err(e.to_string())),
        }
    }
    tx.commit()?;
    Ok(results)
}

/// Removes every credit card and every credit card tombstone, in one
/// transaction.
///
/// Deleting the rows alone is not enough. A delete leaves a tombstone behind for
/// any guid the sync mirror knows, and the insert trigger then rejects re-adding
/// that guid, so a wipe that kept them could not be followed by a re-import of
/// the same records. Clearing both tables is what makes the wipe repeatable.
pub(crate) fn delete_all_credit_cards(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM credit_cards_data", [])?;
    // After the data, so the tombstones the delete trigger just created go too.
    tx.execute("DELETE FROM credit_cards_tombstones", [])?;
    tx.commit()?;
    Ok(())
}

/// Adds tombstones for records that were deleted locally but not yet uploaded,
/// within a single transaction and with a result per record. `time_deleted` comes
/// from the caller rather than being stamped as now, so that a deletion imported
/// from another store keeps its original time. Without the tombstone the next
/// sync has nothing to say the record was deleted and takes the server copy.
pub(crate) fn add_many_credit_card_tombstones(
    conn: &Connection,
    tombstones: Vec<(String, i64)>,
) -> Result<Vec<std::result::Result<String, String>>> {
    let tx = conn.unchecked_transaction()?;
    let mut results = Vec::with_capacity(tombstones.len());
    for (guid, time_deleted) in tombstones {
        let inserted = with_savepoint(&tx, || {
            tx.execute(
                "INSERT INTO credit_cards_tombstones (guid, time_deleted)
                 VALUES (:guid, :time_deleted)",
                rusqlite::named_params! {
                    ":guid": &guid,
                    ":time_deleted": timestamp_from_millis(time_deleted),
                },
            )?;
            Ok(())
        })?;
        match inserted {
            Ok(()) => results.push(Ok(guid)),
            Err(e) => results.push(Err(e.to_string())),
        }
    }
    tx.commit()?;
    Ok(results)
}

fn internal_credit_card_from_meta(
    fields: UpdatableCreditCardFields,
    meta: &CreditCardMeta,
) -> InternalCreditCard {
    InternalCreditCard {
        guid: Guid::new(&meta.guid),
        cc_name: fields.cc_name,
        cc_number_enc: fields.cc_number_enc,
        cc_number_last_4: fields.cc_number_last_4,
        cc_exp_month: fields.cc_exp_month,
        cc_exp_year: fields.cc_exp_year,
        cc_type: fields.cc_type,
        metadata: Metadata {
            time_created: timestamp_from_millis(meta.time_created),
            time_last_used: timestamp_from_millis(meta.time_last_used.unwrap_or(0)),
            time_last_modified: timestamp_from_millis(meta.time_last_modified),
            times_used: meta.times_used,
            sync_change_counter: meta.sync_change_counter,
        },
    }
}

/// Updates a credit card **including metadata**, setting both its fields and its
/// timestamps and `times_used` to the supplied values. Normally you will use
/// `update_credit_card` instead, which owns the metadata itself; this is for
/// keeping a record identical to one held in another store. Errors with
/// `NoSuchRecord` if the guid is absent.
pub(crate) fn update_credit_card_with_meta(
    conn: &Connection,
    fields: UpdatableCreditCardFields,
    meta: CreditCardMeta,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    let card = internal_credit_card_from_meta(fields, &meta);
    // Checked up front because `update_internal_credit_card` does not report
    // how many rows it changed.
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM credit_cards_data WHERE guid = :guid)",
        rusqlite::named_params! { ":guid": card.guid },
        |row| row.get(0),
    )?;
    if !exists {
        return Err(Error::NoSuchRecord(card.guid.to_string()));
    }
    update_internal_credit_card(
        &tx,
        &card,
        CounterUpdate::Set(card.metadata.sync_change_counter),
    )?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn add_internal_credit_card(
    tx: &Transaction<'_>,
    card: &InternalCreditCard,
) -> Result<()> {
    tx.execute(
        &format!(
            "INSERT INTO credit_cards_data (
                {common_cols},
                sync_change_counter
            ) VALUES (
                {common_vals},
                :sync_change_counter
            )",
            common_cols = CREDIT_CARD_COMMON_COLS,
            common_vals = CREDIT_CARD_COMMON_VALS,
        ),
        rusqlite::named_params! {
            ":guid": card.guid,
            ":cc_name": card.cc_name,
            ":cc_number_enc": card.cc_number_enc,
            ":cc_number_last_4": card.cc_number_last_4,
            ":cc_exp_month": card.cc_exp_month,
            ":cc_exp_year": card.cc_exp_year,
            ":cc_type": card.cc_type,
            ":time_created": card.metadata.time_created,
            ":time_last_used": card.metadata.time_last_used,
            ":time_last_modified": card.metadata.time_last_modified,
            ":times_used": card.metadata.times_used,
            ":sync_change_counter": card.metadata.sync_change_counter,
        },
    )?;
    Ok(())
}

pub(crate) fn get_credit_card(conn: &Connection, guid: &Guid) -> Result<InternalCreditCard> {
    let sql = format!(
        "SELECT
            {common_cols},
            sync_change_counter
        FROM credit_cards_data
        WHERE guid = :guid",
        common_cols = CREDIT_CARD_COMMON_COLS
    );

    conn.query_row(&sql, [guid], InternalCreditCard::from_row)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NoSuchRecord(guid.to_string()),
            e => e.into(),
        })
}

pub(crate) fn get_all_credit_cards(conn: &Connection) -> Result<Vec<InternalCreditCard>> {
    let sql = format!(
        "SELECT
            {common_cols},
            sync_change_counter
        FROM credit_cards_data",
        common_cols = CREDIT_CARD_COMMON_COLS
    );

    let mut stmt = conn.prepare(&sql)?;
    let credit_cards = stmt
        .query_map([], InternalCreditCard::from_row)?
        .collect::<std::result::Result<Vec<InternalCreditCard>, _>>()?;
    Ok(credit_cards)
}

pub(crate) fn count_all_credit_cards(conn: &Connection) -> Result<i64> {
    let sql = "SELECT COUNT(*)
        FROM credit_cards_data";

    let mut stmt = conn.prepare(sql)?;
    let count: i64 = stmt.query_row([], |row| row.get(0))?;
    Ok(count)
}

pub fn update_credit_card(
    conn: &Connection,
    guid: &Guid,
    credit_card: &UpdatableCreditCardFields,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE credit_cards_data
        SET cc_name                     = :cc_name,
            cc_number_enc               = :cc_number_enc,
            cc_number_last_4            = :cc_number_last_4,
            cc_exp_month                = :cc_exp_month,
            cc_exp_year                 = :cc_exp_year,
            cc_type                     = :cc_type,
            time_last_modified          = :time_last_modified,
            sync_change_counter         = sync_change_counter + 1
        WHERE guid                      = :guid",
        rusqlite::named_params! {
            ":cc_name": credit_card.cc_name,
            ":cc_number_enc": credit_card.cc_number_enc,
            ":cc_number_last_4": credit_card.cc_number_last_4,
            ":cc_exp_month": credit_card.cc_exp_month,
            ":cc_exp_year": credit_card.cc_exp_year,
            ":cc_type": credit_card.cc_type,
            ":time_last_modified": Timestamp::now(),
            ":guid": guid,
        },
    )?;

    tx.commit()?;
    Ok(())
}

/// Updates all fields including metadata - although the change counter gets
/// slightly special treatment, see `CounterUpdate`.
pub(crate) fn update_internal_credit_card(
    tx: &Transaction<'_>,
    card: &InternalCreditCard,
    counter: CounterUpdate,
) -> Result<()> {
    let (counter_sql, counter_value) = counter.as_sql();
    tx.execute(
        &format!(
            "UPDATE credit_cards_data
        SET cc_name                     = :cc_name,
            cc_number_enc               = :cc_number_enc,
            cc_number_last_4            = :cc_number_last_4,
            cc_exp_month                = :cc_exp_month,
            cc_exp_year                 = :cc_exp_year,
            cc_type                     = :cc_type,
            time_created                = :time_created,
            time_last_used              = :time_last_used,
            time_last_modified          = :time_last_modified,
            times_used                  = :times_used,
            sync_change_counter         = {counter_sql}
        WHERE guid                      = :guid"
        ),
        rusqlite::named_params! {
            ":cc_name": card.cc_name,
            ":cc_number_enc": card.cc_number_enc,
            ":cc_number_last_4": card.cc_number_last_4,
            ":cc_exp_month": card.cc_exp_month,
            ":cc_exp_year": card.cc_exp_year,
            ":cc_type": card.cc_type,
            ":time_created": card.metadata.time_created,
            ":time_last_used": card.metadata.time_last_used,
            ":time_last_modified": card.metadata.time_last_modified,
            ":times_used": card.metadata.times_used,
            ":counter": counter_value,
            ":guid": card.guid,
        },
    )?;
    Ok(())
}

pub fn delete_credit_card(conn: &Connection, guid: &Guid) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;

    // execute returns how many rows were affected.
    let exists = tx.execute(
        "DELETE FROM credit_cards_data
        WHERE guid = :guid",
        rusqlite::named_params! {
            ":guid": guid.as_str(),
        },
    )? != 0;

    tx.commit()?;
    Ok(exists)
}

pub fn scrub_encrypted_credit_card_data(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE credit_cards_data SET cc_number_enc = ''", [])?;
    tx.commit()?;
    Ok(())
}

pub fn scrub_undecryptable_credit_card_data_for_remote_replacement(
    conn: &Connection,
    local_encryption_key: String,
) -> Result<CreditCardsDeletionMetrics> {
    let tx = conn.unchecked_transaction()?;
    let mut scrubbed_records = 0;
    let encdec = EncryptorDecryptor::new(local_encryption_key.as_str()).unwrap();

    let undecryptable_record_ids = get_all_credit_cards(conn)?
        .into_iter()
        .filter(|credit_card| encdec.decrypt(&credit_card.cc_number_enc).is_err())
        .map(|credit_card| credit_card.guid)
        .collect::<Vec<_>>();

    // Reset the cc_number_enc field as well as the meta fields of the record so if the record was previously synced
    // it will be overwritten
    sql_support::each_chunk(&undecryptable_record_ids, |chunk, _| -> Result<()> {
        let scrubbed = tx.execute(
            &format!(
                "UPDATE credit_cards_data
                SET cc_number_enc = '',
                    time_created = 0,
                    time_last_used = 0,
                    time_last_modified = 0,
                    times_used = 0,
                    sync_change_counter = 0
                WHERE guid IN ({})",
                sql_support::repeat_sql_values(chunk.len())
            ),
            rusqlite::params_from_iter(chunk),
        )?;
        scrubbed_records += scrubbed;
        Ok(())
    })?;

    tx.commit()?;
    Ok(CreditCardsDeletionMetrics {
        total_scrubbed_records: scrubbed_records as u64,
    })
}

pub fn touch(conn: &Connection, guid: &Guid) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let now_ms = Timestamp::now();

    tx.execute(
        "UPDATE credit_cards_data
        SET time_last_used              = :time_last_used,
            times_used                  = times_used + 1,
            sync_change_counter         = sync_change_counter + 1
        WHERE guid                      = :guid",
        rusqlite::named_params! {
            ":time_last_used": now_ms,
            ":guid": guid.as_str(),
        },
    )?;

    tx.commit()?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::test::new_mem_db;
    use crate::encryption::EncryptorDecryptor;
    use nss_as::ensure_initialized;
    use sync15::bso::IncomingBso;

    fn meta_test_fields(cc_name: &str) -> UpdatableCreditCardFields {
        UpdatableCreditCardFields {
            cc_name: cc_name.to_string(),
            // The `credit_cards_data` CHECK constraint requires either an empty
            // string or more than 20 characters, real ciphertext being long.
            cc_number_enc: "0123456789012345678901234567890".to_string(),
            cc_number_last_4: "1234".to_string(),
            cc_exp_month: 4,
            cc_exp_year: 2030,
            cc_type: "visa".to_string(),
        }
    }

    fn meta_test_meta(guid: &str, sync_change_counter: i64) -> CreditCardMeta {
        CreditCardMeta {
            guid: guid.to_string(),
            time_created: 1000,
            time_last_used: Some(2000),
            time_last_modified: 3000,
            times_used: 4,
            sync_change_counter,
        }
    }

    fn count_cc_tombstones(conn: &Connection, guid: &str) -> Result<i64> {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM credit_cards_tombstones WHERE guid = :guid",
            rusqlite::named_params! { ":guid": guid },
            |row| row.get(0),
        )?)
    }

    #[test]
    fn test_credit_card_add_with_meta() -> Result<()> {
        let db = new_mem_db();

        let saved =
            add_credit_card_with_meta(&db, meta_test_fields("Jane Doe"), meta_test_meta("abc", 2))?;

        // the supplied guid is used rather than a fresh one being generated.
        assert_eq!(saved.guid.as_str(), "abc");

        let retrieved = get_credit_card(&db, &Guid::new("abc"))?;
        assert_eq!(retrieved.cc_name, "Jane Doe");
        assert_eq!(retrieved.metadata.time_created.as_millis(), 1000);
        assert_eq!(retrieved.metadata.time_last_used.as_millis(), 2000);
        assert_eq!(retrieved.metadata.time_last_modified.as_millis(), 3000);
        assert_eq!(retrieved.metadata.times_used, 4);
        assert_eq!(retrieved.metadata.sync_change_counter, 2);

        Ok(())
    }

    #[test]
    fn test_credit_card_add_with_meta_sanitizes_out_of_range_timestamps() -> Result<()> {
        let db = new_mem_db();

        // Negative, and the value from bug 2066257 - a negative microsecond
        // timestamp that a JS consumer already reinterpreted as a u64 and
        // divided by 1000, so it reaches us as a huge positive number. Both are
        // "we don't know when", and a `.max(0)` would only catch the first.
        for (guid, out_of_range) in [("abc", -1), ("def", 18446744071857664)] {
            let meta = CreditCardMeta {
                guid: guid.to_string(),
                time_created: out_of_range,
                time_last_used: Some(out_of_range),
                time_last_modified: out_of_range,
                times_used: 0,
                sync_change_counter: 0,
            };
            add_credit_card_with_meta(&db, meta_test_fields("Jane Doe"), meta)?;

            let retrieved = get_credit_card(&db, &Guid::new(guid))?;
            assert_eq!(
                retrieved.metadata.time_created.as_millis(),
                0,
                "{out_of_range} survived"
            );
            assert_eq!(retrieved.metadata.time_last_used.as_millis(), 0);
            assert_eq!(retrieved.metadata.time_last_modified.as_millis(), 0);
        }

        Ok(())
    }

    /// Surface 2: a value already on disk, put there before the import path
    /// sanitized anything. Reading it must repair rather than propagate it.
    #[test]
    fn test_credit_card_from_row_sanitizes_corrupt_timestamps() -> Result<()> {
        let db = new_mem_db();

        let card = add_credit_card(&db, meta_test_fields("Jane Doe"))?;
        db.execute(
            // Three shapes that are not representable dates: the u64-reinterpreted
            // value from bug 2066257, a raw negative, and MAX_DATE_MS + 1.
            "UPDATE credit_cards_data
             SET time_created = 18446744071857664,
                 time_last_used = -1,
                 time_last_modified = 8640000000000001
             WHERE guid = :guid",
            rusqlite::named_params! { ":guid": card.guid },
        )?;

        let retrieved = get_credit_card(&db, &card.guid)?;
        assert_eq!(retrieved.metadata.time_created.as_millis(), 0);
        assert_eq!(retrieved.metadata.time_last_used.as_millis(), 0);
        assert_eq!(retrieved.metadata.time_last_modified.as_millis(), 0);

        Ok(())
    }

    #[test]
    fn test_credit_card_update_with_meta_keeps_supplied_counter() -> Result<()> {
        let db = new_mem_db();

        add_credit_card_with_meta(&db, meta_test_fields("Jane Doe"), meta_test_meta("abc", 0))?;

        // the supplied counter must be applied, not the one already in the row.
        update_credit_card_with_meta(
            &db,
            meta_test_fields("Jane Q. Doe"),
            meta_test_meta("abc", 1),
        )?;

        let retrieved = get_credit_card(&db, &Guid::new("abc"))?;
        assert_eq!(retrieved.cc_name, "Jane Q. Doe");
        assert_eq!(retrieved.metadata.sync_change_counter, 1);

        // and back down again.
        update_credit_card_with_meta(
            &db,
            meta_test_fields("Jane Q. Doe"),
            meta_test_meta("abc", 0),
        )?;
        assert_eq!(
            get_credit_card(&db, &Guid::new("abc"))?
                .metadata
                .sync_change_counter,
            0
        );

        Ok(())
    }

    #[test]
    fn test_credit_card_update_with_meta_errors_when_missing() -> Result<()> {
        let db = new_mem_db();

        let result = update_credit_card_with_meta(
            &db,
            meta_test_fields("Jane Doe"),
            meta_test_meta("abc", 3),
        );
        assert!(matches!(result, Err(Error::NoSuchRecord(guid)) if guid == "abc"));
        assert!(get_credit_card(&db, &Guid::new("abc")).is_err());

        Ok(())
    }

    #[test]
    fn test_credit_card_add_many_with_meta_isolates_failures() -> Result<()> {
        let db = new_mem_db();

        // the second entry has an empty guid, which the `credit_cards_data`
        // CHECK constraint rejects. The others must still be inserted.
        let results = add_many_credit_cards_with_meta(
            &db,
            vec![
                UpdatableCreditCardFieldsWithMeta {
                    fields: meta_test_fields("One"),
                    meta: meta_test_meta("aaa", 1),
                },
                UpdatableCreditCardFieldsWithMeta {
                    fields: meta_test_fields("Two"),
                    meta: meta_test_meta("", 1),
                },
                UpdatableCreditCardFieldsWithMeta {
                    fields: meta_test_fields("Three"),
                    meta: meta_test_meta("ccc", 1),
                },
            ],
        )?;

        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
        assert!(results[2].is_ok());
        assert_eq!(get_all_credit_cards(&db)?.len(), 2);

        Ok(())
    }

    #[test]
    fn test_delete_all_credit_cards_allows_a_reimport() -> Result<()> {
        let db = new_mem_db();

        // A tombstone left by an earlier import, and a record sharing no guid
        // with it.
        add_many_credit_card_tombstones(&db, vec![("gone".to_string(), 1234)])?;
        let card = add_credit_card(&db, meta_test_fields("Jane Doe"))?;

        delete_all_credit_cards(&db)?;
        assert_eq!(get_all_credit_cards(&db)?.len(), 0);
        let tombstones: i64 =
            db.query_row("SELECT COUNT(*) FROM credit_cards_tombstones", [], |row| {
                row.get(0)
            })?;
        assert_eq!(tombstones, 0, "tombstones are cleared with the records");

        // The point of clearing them: re-importing the same guids succeeds,
        // where the insert trigger would reject a guid still tombstoned.
        let results = add_many_credit_cards_with_meta(
            &db,
            vec![
                UpdatableCreditCardFieldsWithMeta {
                    fields: meta_test_fields("Jane Doe"),
                    meta: CreditCardMeta {
                        guid: card.guid.to_string(),
                        ..Default::default()
                    },
                },
                UpdatableCreditCardFieldsWithMeta {
                    fields: meta_test_fields("Gone"),
                    meta: CreditCardMeta {
                        guid: "gone".to_string(),
                        ..Default::default()
                    },
                },
            ],
        )?;
        assert!(
            results.iter().all(|r| r.is_ok()),
            "a previously tombstoned guid can be re-imported: {results:?}"
        );

        Ok(())
    }

    #[test]
    fn test_credit_card_add_many_tombstones() -> Result<()> {
        let db = new_mem_db();

        let results = add_many_credit_card_tombstones(&db, vec![("aaa".to_string(), 1234)])?;
        assert_eq!(results.len(), 1);
        assert!(results[0].is_ok());

        // the supplied deletion time is used rather than being stamped as now.
        let time_deleted: i64 = db.query_row(
            "SELECT time_deleted FROM credit_cards_tombstones WHERE guid = 'aaa'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(time_deleted, 1234);

        Ok(())
    }

    #[test]
    fn test_credit_card_add_many_tombstones_rejects_live_guid() -> Result<()> {
        let db = new_mem_db();

        add_credit_card_with_meta(&db, meta_test_fields("Jane Doe"), meta_test_meta("abc", 0))?;

        // a guid cannot be in both `credit_cards_data` and
        // `credit_cards_tombstones`; the trigger enforcing that must not take
        // the rest of the batch down.
        let results = add_many_credit_card_tombstones(
            &db,
            vec![("abc".to_string(), 1234), ("ddd".to_string(), 5678)],
        )?;

        assert_eq!(results.len(), 2);
        assert!(results[0].is_err());
        assert!(results[1].is_ok());

        // the rejected tombstone must not have been committed anyway - see
        // `with_savepoint`.
        assert_eq!(count_cc_tombstones(&db, "abc")?, 0);
        assert!(get_credit_card(&db, &Guid::new("abc")).is_ok());
        assert_eq!(count_cc_tombstones(&db, "ddd")?, 1);

        Ok(())
    }

    #[test]
    fn test_credit_card_add_many_with_meta_rejects_deleted_guid() -> Result<()> {
        let db = new_mem_db();

        add_many_credit_card_tombstones(&db, vec![("aaa".to_string(), 1234)])?;

        // the other side of the same invariant: a guid in
        // `credit_cards_tombstones` cannot be inserted into
        // `credit_cards_data`.
        let results = add_many_credit_cards_with_meta(
            &db,
            vec![
                UpdatableCreditCardFieldsWithMeta {
                    fields: meta_test_fields("One"),
                    meta: meta_test_meta("aaa", 1),
                },
                UpdatableCreditCardFieldsWithMeta {
                    fields: meta_test_fields("Two"),
                    meta: meta_test_meta("bbb", 1),
                },
            ],
        )?;

        assert_eq!(results.len(), 2);
        assert!(results[0].is_err());
        assert!(results[1].is_ok());

        assert!(get_credit_card(&db, &Guid::new("aaa")).is_err());
        assert_eq!(get_all_credit_cards(&db)?.len(), 1);

        Ok(())
    }

    pub fn get_all(
        conn: &Connection,
        table_name: String,
    ) -> rusqlite::Result<Vec<String>, rusqlite::Error> {
        let mut stmt = conn.prepare(&format!(
            "SELECT guid FROM {table_name}",
            table_name = table_name
        ))?;
        let rows = stmt.query_map([], |row| row.get(0))?;

        let mut guids = Vec::new();
        for guid_result in rows {
            guids.push(guid_result?);
        }

        Ok(guids)
    }

    pub fn insert_tombstone_record(
        conn: &Connection,
        guid: String,
    ) -> rusqlite::Result<usize, rusqlite::Error> {
        conn.execute(
            "INSERT INTO credit_cards_tombstones (
                guid,
                time_deleted
            ) VALUES (
                :guid,
                :time_deleted
            )",
            rusqlite::named_params! {
                ":guid": guid,
                ":time_deleted": Timestamp::now(),
            },
        )
    }

    pub(crate) fn test_insert_mirror_record(conn: &Connection, bso: IncomingBso) {
        // This test function is a bit suspect, because credit-cards always
        // store encrypted records, which this ignores entirely, and stores the
        // raw payload with a cleartext cc_number.
        // It's OK for all current test consumers, but it's a bit of a smell...
        conn.execute(
            "INSERT INTO credit_cards_mirror (guid, payload)
             VALUES (:guid, :payload)",
            rusqlite::named_params! {
                ":guid": &bso.envelope.id,
                ":payload": &bso.payload,
            },
        )
        .expect("should insert");
    }

    #[test]
    fn test_credit_card_create_and_read() -> Result<()> {
        ensure_initialized();
        let db = new_mem_db();

        let saved_credit_card = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "jane doe".to_string(),
                cc_number_enc: "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX".to_string(),
                cc_number_last_4: "1234".to_string(),
                cc_exp_month: 3,
                cc_exp_year: 2022,
                cc_type: "visa".to_string(),
            },
        )?;

        // check that the add function populated the guid field
        assert_ne!(Guid::default(), saved_credit_card.guid);

        // check that the time created and time last modified were set
        assert_ne!(0, saved_credit_card.metadata.time_created.as_millis());
        assert_ne!(0, saved_credit_card.metadata.time_last_modified.as_millis());

        // check that sync_change_counter was set to 0.
        assert_eq!(0, saved_credit_card.metadata.sync_change_counter);

        // get created credit card
        let retrieved_credit_card = get_credit_card(&db, &saved_credit_card.guid)?;

        assert_eq!(saved_credit_card.guid, retrieved_credit_card.guid);
        assert_eq!(saved_credit_card.cc_name, retrieved_credit_card.cc_name);
        assert_eq!(
            saved_credit_card.cc_number_enc,
            retrieved_credit_card.cc_number_enc
        );
        assert_eq!(
            saved_credit_card.cc_number_last_4,
            retrieved_credit_card.cc_number_last_4
        );
        assert_eq!(
            saved_credit_card.cc_exp_month,
            retrieved_credit_card.cc_exp_month
        );
        assert_eq!(
            saved_credit_card.cc_exp_year,
            retrieved_credit_card.cc_exp_year
        );
        assert_eq!(saved_credit_card.cc_type, retrieved_credit_card.cc_type);

        // converting the created record into a tombstone to check that it's not returned on a second `get_credit_card` call
        let delete_result = delete_credit_card(&db, &saved_credit_card.guid);
        assert!(delete_result.is_ok());
        assert!(delete_result?);

        assert!(get_credit_card(&db, &saved_credit_card.guid).is_err());

        Ok(())
    }

    #[test]
    fn test_credit_card_missing_guid() {
        ensure_initialized();
        let db = new_mem_db();
        let guid = Guid::random();
        let result = get_credit_card(&db, &guid);

        assert_eq!(
            result.unwrap_err().to_string(),
            Error::NoSuchRecord(guid.to_string()).to_string()
        );
    }

    #[test]
    fn test_credit_card_read_all() -> Result<()> {
        ensure_initialized();
        let db = new_mem_db();

        let saved_credit_card = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "jane doe".to_string(),
                cc_number_enc: "YYYYYYYYYYYYYYYYYYYYYYYYYYYYY".to_string(),
                cc_number_last_4: "4321".to_string(),
                cc_exp_month: 3,
                cc_exp_year: 2022,
                cc_type: "visa".to_string(),
            },
        )?;

        let saved_credit_card2 = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "john deer".to_string(),
                cc_number_enc: "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ".to_string(),
                cc_number_last_4: "6543".to_string(),
                cc_exp_month: 10,
                cc_exp_year: 2025,
                cc_type: "mastercard".to_string(),
            },
        )?;

        // creating a third credit card with a tombstone to ensure it's not returned
        let saved_credit_card3 = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "abraham lincoln".to_string(),
                cc_number_enc: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
                cc_number_last_4: "9876".to_string(),
                cc_exp_month: 1,
                cc_exp_year: 2024,
                cc_type: "amex".to_string(),
            },
        )?;

        let delete_result = delete_credit_card(&db, &saved_credit_card3.guid);
        assert!(delete_result.is_ok());
        assert!(delete_result?);

        let retrieved_credit_cards = get_all_credit_cards(&db)?;

        assert!(!retrieved_credit_cards.is_empty());
        let expected_number_of_credit_cards = 2;
        assert_eq!(
            expected_number_of_credit_cards,
            retrieved_credit_cards.len()
        );

        let credit_card_count = count_all_credit_cards(&db)?;
        assert_eq!(expected_number_of_credit_cards, credit_card_count as usize);

        let retrieved_credit_card_guids = [
            retrieved_credit_cards[0].guid.as_str(),
            retrieved_credit_cards[1].guid.as_str(),
        ];
        assert!(retrieved_credit_card_guids.contains(&saved_credit_card.guid.as_str()));
        assert!(retrieved_credit_card_guids.contains(&saved_credit_card2.guid.as_str()));

        Ok(())
    }

    #[test]
    fn test_credit_card_update() -> Result<()> {
        ensure_initialized();
        let db = new_mem_db();

        let saved_credit_card = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "john deer".to_string(),
                cc_number_enc: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
                cc_number_last_4: "4321".to_string(),
                cc_exp_month: 10,
                cc_exp_year: 2025,
                cc_type: "mastercard".to_string(),
            },
        )?;

        let expected_cc_name = "john doe".to_string();
        let update_result = update_credit_card(
            &db,
            &saved_credit_card.guid,
            &UpdatableCreditCardFields {
                cc_name: expected_cc_name.clone(),
                cc_number_enc: "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string(),
                cc_number_last_4: "1234".to_string(),
                cc_type: "mastercard".to_string(),
                cc_exp_month: 10,
                cc_exp_year: 2025,
            },
        );
        assert!(update_result.is_ok());

        let updated_credit_card = get_credit_card(&db, &saved_credit_card.guid)?;

        assert_eq!(saved_credit_card.guid, updated_credit_card.guid);
        assert_eq!(expected_cc_name, updated_credit_card.cc_name);

        //check that the sync_change_counter was incremented
        assert_eq!(1, updated_credit_card.metadata.sync_change_counter);

        Ok(())
    }

    #[test]
    fn test_credit_card_update_internal_credit_card() -> Result<()> {
        ensure_initialized();
        let mut db = new_mem_db();
        let tx = db.transaction()?;

        let guid = Guid::random();
        add_internal_credit_card(
            &tx,
            &InternalCreditCard {
                guid: guid.clone(),
                cc_name: "john deer".to_string(),
                cc_number_enc: "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string(),
                cc_number_last_4: "1234".to_string(),
                cc_exp_month: 10,
                cc_exp_year: 2025,
                cc_type: "mastercard".to_string(),
                ..Default::default()
            },
        )?;

        let expected_cc_exp_month = 11;
        update_internal_credit_card(
            &tx,
            &InternalCreditCard {
                guid: guid.clone(),
                cc_name: "john deer".to_string(),
                cc_number_enc: "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string(),
                cc_number_last_4: "1234".to_string(),
                cc_exp_month: expected_cc_exp_month,
                cc_exp_year: 2025,
                cc_type: "mastercard".to_string(),
                ..Default::default()
            },
            CounterUpdate::Leave,
        )?;

        let record_exists: bool = tx.query_row(
            "SELECT EXISTS (
                SELECT 1
                FROM credit_cards_data
                WHERE guid = :guid
                AND cc_exp_month = :cc_exp_month
                AND sync_change_counter = 0
            )",
            [&guid.to_string(), &expected_cc_exp_month.to_string()],
            |row| row.get(0),
        )?;
        assert!(record_exists);

        Ok(())
    }

    #[test]
    fn test_credit_card_delete() -> Result<()> {
        ensure_initialized();
        let db = new_mem_db();
        let encdec = EncryptorDecryptor::new_with_random_key().unwrap();

        let saved_credit_card = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "john deer".to_string(),
                cc_number_enc: encdec.encrypt("1234567812345678")?,
                cc_number_last_4: "5678".to_string(),
                cc_exp_month: 10,
                cc_exp_year: 2025,
                cc_type: "mastercard".to_string(),
            },
        )?;

        let delete_result = delete_credit_card(&db, &saved_credit_card.guid);
        assert!(delete_result.is_ok());
        assert!(delete_result?);

        let saved_credit_card2 = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "john doe".to_string(),
                cc_number_enc: encdec.encrypt("1234123412341234")?,
                cc_number_last_4: "1234".to_string(),
                cc_exp_month: 5,
                cc_exp_year: 2024,
                cc_type: "visa".to_string(),
            },
        )?;

        // create a mirror record to check that a tombstone record is created upon deletion
        let cc2_guid = saved_credit_card2.guid.clone();
        let payload = saved_credit_card2.into_test_incoming_bso(&encdec, Default::default());

        test_insert_mirror_record(&db, payload);

        let delete_result2 = delete_credit_card(&db, &cc2_guid);
        assert!(delete_result2.is_ok());
        assert!(delete_result2?);

        // check that a tombstone record exists since the record existed in the mirror
        let tombstone_exists: bool = db.query_row(
            "SELECT EXISTS (
                SELECT 1
                FROM credit_cards_tombstones
                WHERE guid = :guid
            )",
            [&cc2_guid],
            |row| row.get(0),
        )?;
        assert!(tombstone_exists);

        // remove the tombstone record
        db.execute(
            "DELETE FROM credit_cards_tombstones
            WHERE guid = :guid",
            rusqlite::named_params! {
                ":guid": cc2_guid,
            },
        )?;

        Ok(())
    }

    #[test]
    fn test_scrub_encrypted_credit_card_data() -> Result<()> {
        ensure_initialized();
        let db = new_mem_db();
        let encdec = EncryptorDecryptor::new_with_random_key().unwrap();
        let mut saved_credit_cards = Vec::with_capacity(10);
        for _ in 0..5 {
            saved_credit_cards.push(add_credit_card(
                &db,
                UpdatableCreditCardFields {
                    cc_name: "john deer".to_string(),
                    cc_number_enc: encdec.encrypt("1234567812345678")?,
                    cc_number_last_4: "5678".to_string(),
                    cc_exp_month: 10,
                    cc_exp_year: 2025,
                    cc_type: "mastercard".to_string(),
                },
            )?);
        }

        scrub_encrypted_credit_card_data(&db)?;
        for saved_credit_card in saved_credit_cards.into_iter() {
            let retrieved_credit_card = get_credit_card(&db, &saved_credit_card.guid)?;
            assert_eq!(retrieved_credit_card.cc_number_enc, "");
        }

        Ok(())
    }

    #[test]
    fn test_scrub_undecryptable_credit_card_date_for_remote_replacement() -> Result<()> {
        ensure_initialized();
        let db = new_mem_db();
        let old_key = EncryptorDecryptor::create_key()?;
        let old_encdec = EncryptorDecryptor::new(&old_key)?;
        let key = EncryptorDecryptor::create_key()?;
        let encdec = EncryptorDecryptor::new(&key)?;

        let undecryptable_credit_card = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "jane doe".to_string(),
                cc_number_enc: old_encdec.encrypt("2345678923456789")?,
                cc_number_last_4: "6789".to_string(),
                cc_exp_month: 9,
                cc_exp_year: 2027,
                cc_type: "visa".to_string(),
            },
        )?;

        let encrypted_cc_number = encdec.encrypt("567812345678123456781")?;
        let credit_card = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "john deer".to_string(),
                cc_number_enc: encrypted_cc_number.clone(),
                cc_number_last_4: "6781".to_string(),
                cc_exp_month: 10,
                cc_exp_year: 2025,
                cc_type: "mastercard".to_string(),
            },
        )?;

        let metrics = scrub_undecryptable_credit_card_data_for_remote_replacement(&db.writer, key)?;
        assert_eq!(metrics.total_scrubbed_records, 1);

        let credit_cards = get_all_credit_cards(&db)?;
        assert_eq!(credit_cards.len(), 2);

        let retrieved_credit_card = get_credit_card(&db, &undecryptable_credit_card.guid)?;
        assert_eq!(retrieved_credit_card.cc_number_enc, "");

        let retrieved_credit_card2 = get_credit_card(&db, &credit_card.guid)?;
        assert_eq!(retrieved_credit_card2.cc_number_enc, encrypted_cc_number);

        Ok(())
    }

    #[test]
    fn test_credit_card_trigger_on_create() -> Result<()> {
        ensure_initialized();
        let db = new_mem_db();
        let tx = db.unchecked_transaction()?;
        let guid = Guid::random();

        // create a tombstone record
        insert_tombstone_record(&db, guid.to_string())?;

        // create a new credit card with the tombstone's guid
        let credit_card = InternalCreditCard {
            guid,
            cc_name: "john deer".to_string(),
            cc_number_enc: "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW".to_string(),
            cc_number_last_4: "6543".to_string(),
            cc_exp_month: 10,
            cc_exp_year: 2025,
            cc_type: "mastercard".to_string(),

            ..Default::default()
        };

        let add_credit_card_result = add_internal_credit_card(&tx, &credit_card);
        assert!(add_credit_card_result.is_err());

        let expected_error_message = "guid exists in `credit_cards_tombstones`";
        assert!(add_credit_card_result
            .unwrap_err()
            .to_string()
            .contains(expected_error_message));

        Ok(())
    }

    #[test]
    fn test_credit_card_trigger_on_delete() -> Result<()> {
        ensure_initialized();
        let db = new_mem_db();
        let tx = db.unchecked_transaction()?;
        let guid = Guid::random();

        // create an credit card
        let credit_card = InternalCreditCard {
            guid,
            cc_name: "jane doe".to_string(),
            cc_number_enc: "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW".to_string(),
            cc_number_last_4: "6543".to_string(),
            cc_exp_month: 3,
            cc_exp_year: 2022,
            cc_type: "visa".to_string(),
            ..Default::default()
        };
        add_internal_credit_card(&tx, &credit_card)?;

        // create a tombstone record with the same guid
        let tombstone_result = insert_tombstone_record(&db, credit_card.guid.to_string());

        let expected_error_message = "guid exists in `credit_cards_data`";
        assert!(tombstone_result
            .unwrap_err()
            .to_string()
            .contains(expected_error_message));

        Ok(())
    }

    #[test]
    fn test_credit_card_touch() -> Result<()> {
        ensure_initialized();
        let db = new_mem_db();
        let saved_credit_card = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "john doe".to_string(),
                cc_number_enc: "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW".to_string(),
                cc_number_last_4: "6543".to_string(),
                cc_exp_month: 5,
                cc_exp_year: 2024,
                cc_type: "visa".to_string(),
            },
        )?;

        assert_eq!(saved_credit_card.metadata.sync_change_counter, 0);
        assert_eq!(saved_credit_card.metadata.times_used, 0);

        touch(&db, &saved_credit_card.guid)?;

        let touched_credit_card = get_credit_card(&db, &saved_credit_card.guid)?;

        assert_eq!(touched_credit_card.metadata.sync_change_counter, 1);
        assert_eq!(touched_credit_card.metadata.times_used, 1);

        Ok(())
    }
}

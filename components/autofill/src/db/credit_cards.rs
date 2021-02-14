/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::{
    models::credit_card::{InternalCreditCard, UpdatableCreditCardFields},
    schema::{CREDIT_CARD_COMMON_COLS, CREDIT_CARD_COMMON_VALS},
    store::{delete_meta, put_meta},
};
use crate::error::*;
use crate::sync::credit_card::engine::{
    COLLECTION_SYNCID_META_KEY, GLOBAL_SYNCID_META_KEY, LAST_SYNC_META_KEY,
};

use rusqlite::{Connection, Transaction, NO_PARAMS};
use sync15::EngineSyncAssociation;
use sync_guid::Guid;
use types::Timestamp;

pub fn add_credit_card(
    conn: &Connection,
    new_credit_card_fields: UpdatableCreditCardFields,
) -> Result<InternalCreditCard> {
    let tx = conn.unchecked_transaction()?;

    // We return an InternalCreditCard, so set it up first, including the
    // missing fields, before we insert it.
    let credit_card = InternalCreditCard {
        guid: Guid::random(),
        cc_name: new_credit_card_fields.cc_name,
        cc_number: new_credit_card_fields.cc_number,
        cc_exp_month: new_credit_card_fields.cc_exp_month,
        cc_exp_year: new_credit_card_fields.cc_exp_year,
        // Credit card types are a fixed set of strings as defined in the link below
        // (https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
        cc_type: new_credit_card_fields.cc_type,
        time_created: Timestamp::now(),
        time_last_used: Some(Timestamp::now()),
        time_last_modified: Timestamp::now(),
        times_used: 0,
        sync_change_counter: 1,
    };

    tx.execute_named(
        &format!(
            "INSERT OR IGNORE INTO credit_cards_data (
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
            ":guid": credit_card.guid,
            ":cc_name": credit_card.cc_name,
            ":cc_number": credit_card.cc_number,
            ":cc_exp_month": credit_card.cc_exp_month,
            ":cc_exp_year": credit_card.cc_exp_year,
            ":cc_type": credit_card.cc_type,
            ":time_created": credit_card.time_created,
            ":time_last_used": credit_card.time_last_used,
            ":time_last_modified": credit_card.time_last_modified,
            ":times_used": credit_card.times_used,
            ":sync_change_counter": credit_card.sync_change_counter,
        },
    )?;

    tx.commit()?;
    Ok(credit_card)
}

pub fn get_credit_card(conn: &Connection, guid: &Guid) -> Result<InternalCreditCard> {
    let tx = conn.unchecked_transaction()?;
    let sql = format!(
        "SELECT
            {common_cols},
            sync_change_counter
        FROM credit_cards_data
        WHERE guid = :guid",
        common_cols = CREDIT_CARD_COMMON_COLS
    );

    let credit_card = tx.query_row(&sql, &[guid], InternalCreditCard::from_row)?;

    tx.commit()?;
    Ok(credit_card)
}

pub fn get_all_credit_cards(conn: &Connection) -> Result<Vec<InternalCreditCard>> {
    let credit_cards;
    let sql = format!(
        "SELECT
            {common_cols},
            sync_change_counter
        FROM credit_cards_data",
        common_cols = CREDIT_CARD_COMMON_COLS
    );

    let mut stmt = conn.prepare(&sql)?;
    credit_cards = stmt
        .query_map(NO_PARAMS, InternalCreditCard::from_row)?
        .collect::<std::result::Result<Vec<InternalCreditCard>, _>>()?;
    Ok(credit_cards)
}

pub fn update_credit_card(
    conn: &Connection,
    guid: &Guid,
    credit_card: &UpdatableCreditCardFields,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_named(
        "UPDATE credit_cards_data
        SET cc_name                     = :cc_name,
            cc_number                   = :cc_number,
            cc_exp_month                = :cc_exp_month,
            cc_exp_year                 = :cc_exp_year,
            cc_type                     = :cc_type,
            time_last_modified          = :time_last_modified,
            sync_change_counter         = sync_change_counter + 1
        WHERE guid                      = :guid",
        rusqlite::named_params! {
            ":cc_name": credit_card.cc_name,
            ":cc_number": credit_card.cc_number,
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

pub fn delete_credit_card(conn: &Connection, guid: &Guid) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;

    // execute_named returns how many rows were affected.
    let exists = tx.execute_named(
        "DELETE FROM credit_cards_data
        WHERE guid = :guid",
        rusqlite::named_params! {
            ":guid": guid.as_str(),
        },
    )? != 0;

    tx.commit()?;
    Ok(exists)
}

pub fn touch(conn: &Connection, guid: &Guid) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let now_ms = Timestamp::now();

    tx.execute_named(
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

pub fn reset_in_tx(tx: &Transaction<'_>, assoc: &EngineSyncAssociation) -> Result<()> {
    // Remove all synced credit cards and pending tombstones, and mark all
    // local credit cards as new.
    tx.execute_batch(
        "DELETE FROM credit_cards_mirror;

        DELETE FROM credit_cards_tombstones;

        UPDATE credit_cards_data
        SET sync_change_counter = 1",
    )?;

    // Reset the last sync time, so that the next sync fetches fresh records
    // from the server.
    put_meta(tx, LAST_SYNC_META_KEY, &0)?;

    // Clear the sync ID if we're signing out, or set it to whatever the
    // server gave us if we're signing in.
    match assoc {
        EngineSyncAssociation::Disconnected => {
            delete_meta(tx, GLOBAL_SYNCID_META_KEY)?;
            delete_meta(tx, COLLECTION_SYNCID_META_KEY)?;
        }
        EngineSyncAssociation::Connected(ids) => {
            put_meta(tx, GLOBAL_SYNCID_META_KEY, &ids.global)?;
            put_meta(tx, COLLECTION_SYNCID_META_KEY, &ids.coll)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{store::get_meta, test::new_mem_db};
    use sql_support::ConnExt;
    use sync15::CollSyncIds;

    fn get_all(
        conn: &Connection,
        table_name: String,
    ) -> rusqlite::Result<Vec<String>, rusqlite::Error> {
        let mut stmt = conn.prepare(&format!(
            "SELECT guid FROM {table_name}",
            table_name = table_name
        ))?;
        let rows = stmt.query_map(NO_PARAMS, |row| row.get(0))?;

        let mut guids = Vec::new();
        for guid_result in rows {
            guids.push(guid_result?);
        }

        Ok(guids)
    }

    fn clear_tables(conn: &Connection) -> rusqlite::Result<(), rusqlite::Error> {
        conn.execute_all(&[
            "DELETE FROM credit_cards_data;",
            "DELETE FROM credit_cards_mirror;",
            "DELETE FROM credit_cards_tombstones;",
            "DELETE FROM moz_meta;",
        ])
    }

    fn insert_tombstone_record(
        conn: &Connection,
        guid: String,
    ) -> rusqlite::Result<usize, rusqlite::Error> {
        conn.execute_named(
            "INSERT OR IGNORE INTO credit_cards_tombstones (
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

    fn insert_mirror_record(conn: &Connection, credit_card: &InternalCreditCard) {
        let payload = serde_json::to_string(credit_card).expect("is json");
        conn.execute_named(
            "INSERT OR IGNORE INTO credit_cards_mirror (guid, payload)
             VALUES (:guid, :payload)",
            rusqlite::named_params! {
                ":guid": credit_card.guid,
                ":payload": &payload,
            },
        )
        .expect("should insert");
    }

    fn insert_record(
        conn: &Connection,
        credit_card: &InternalCreditCard,
    ) -> rusqlite::Result<usize, rusqlite::Error> {
        conn.execute_named(
            &format!(
                "INSERT OR IGNORE INTO credit_cards_data (
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
                ":guid": credit_card.guid,
                ":cc_name": credit_card.cc_name,
                ":cc_number": credit_card.cc_number,
                ":cc_exp_month": credit_card.cc_exp_month,
                ":cc_exp_year": credit_card.cc_exp_year,
                ":cc_type": credit_card.cc_type,
                ":time_created": credit_card.time_created,
                ":time_last_used": credit_card.time_last_used,
                ":time_last_modified": credit_card.time_last_modified,
                ":times_used": credit_card.times_used,
                ":sync_change_counter": credit_card.sync_change_counter,
            },
        )
    }

    #[test]
    fn test_credit_card_create_and_read() -> Result<()> {
        let db = new_mem_db();

        let saved_credit_card = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "jane doe".to_string(),
                cc_number: "2222333344445555".to_string(),
                cc_exp_month: 3,
                cc_exp_year: 2022,
                cc_type: "visa".to_string(),
            },
        )?;

        // check that the add function populated the guid field
        assert_ne!(Guid::default(), saved_credit_card.guid);

        // check that sync_change_counter was set
        assert_eq!(1, saved_credit_card.sync_change_counter);

        // get created credit card
        let retrieved_credit_card = get_credit_card(&db, &saved_credit_card.guid)?;

        assert_eq!(saved_credit_card.guid, retrieved_credit_card.guid);
        assert_eq!(saved_credit_card.cc_name, retrieved_credit_card.cc_name);
        assert_eq!(saved_credit_card.cc_number, retrieved_credit_card.cc_number);
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
    fn test_credit_card_read_all() -> Result<()> {
        let db = new_mem_db();

        let saved_credit_card = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "jane doe".to_string(),
                cc_number: "2222333344445555".to_string(),
                cc_exp_month: 3,
                cc_exp_year: 2022,
                cc_type: "visa".to_string(),
            },
        )?;

        let saved_credit_card2 = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "john deer".to_string(),
                cc_number: "1111222233334444".to_string(),
                cc_exp_month: 10,
                cc_exp_year: 2025,
                cc_type: "mastercard".to_string(),
            },
        )?;

        // creating a third credit card with a tombstone to ensure it's not retunred
        let saved_credit_card3 = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "abraham lincoln".to_string(),
                cc_number: "1111222233334444".to_string(),
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

        let retrieved_credit_card_guids = vec![
            retrieved_credit_cards[0].guid.as_str(),
            retrieved_credit_cards[1].guid.as_str(),
        ];
        assert!(retrieved_credit_card_guids.contains(&saved_credit_card.guid.as_str()));
        assert!(retrieved_credit_card_guids.contains(&saved_credit_card2.guid.as_str()));

        Ok(())
    }

    #[test]
    fn test_credit_card_update() -> Result<()> {
        let db = new_mem_db();

        let saved_credit_card = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "john deer".to_string(),
                cc_number: "1111222233334444".to_string(),
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
                cc_number: "1111222233334444".to_string(),
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
        assert_eq!(2, updated_credit_card.sync_change_counter);

        Ok(())
    }

    #[test]
    fn test_credit_card_delete() -> Result<()> {
        let db = new_mem_db();

        let saved_credit_card = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "john deer".to_string(),
                cc_number: "1111222233334444".to_string(),
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
                cc_number: "5555666677778888".to_string(),
                cc_exp_month: 5,
                cc_exp_year: 2024,
                cc_type: "visa".to_string(),
            },
        )?;

        // create a mirror record to check that a tombstone record is created upon deletion
        insert_mirror_record(&db, &saved_credit_card2);

        let delete_result2 = delete_credit_card(&db, &saved_credit_card2.guid);
        assert!(delete_result2.is_ok());
        assert!(delete_result2?);

        // check that a tombstone record exists since the record existed in the mirror
        let tombstone_exists: bool = db.query_row(
            "SELECT EXISTS (
                SELECT 1
                FROM credit_cards_tombstones
                WHERE guid = :guid
            )",
            &[&saved_credit_card2.guid.as_str()],
            |row| row.get(0),
        )?;
        assert!(tombstone_exists);

        // remove the tombstone record
        db.execute_named(
            "DELETE FROM credit_cards_tombstones
            WHERE guid = :guid",
            rusqlite::named_params! {
                ":guid": saved_credit_card2.guid,
            },
        )?;

        Ok(())
    }

    #[test]
    fn test_credit_card_trigger_on_create() -> Result<()> {
        let db = new_mem_db();
        let guid = Guid::random();

        // create a tombstone record
        insert_tombstone_record(&db, guid.to_string())?;

        // create a new credit card with the tombstone's guid
        let credit_card = InternalCreditCard {
            guid,
            cc_name: "john deer".to_string(),
            cc_number: "1111222233334444".to_string(),
            cc_exp_month: 10,
            cc_exp_year: 2025,
            cc_type: "mastercard".to_string(),

            ..Default::default()
        };

        let add_credit_card_result = insert_record(&db, &credit_card);
        assert!(add_credit_card_result.is_err());

        let expected_error_message = "guid exists in `credit_cards_tombstones`";
        assert_eq!(
            expected_error_message,
            add_credit_card_result.unwrap_err().to_string()
        );

        Ok(())
    }

    #[test]
    fn test_credit_card_trigger_on_delete() -> Result<()> {
        let db = new_mem_db();
        let guid = Guid::random();

        // create an credit card
        let credit_card = InternalCreditCard {
            guid,
            cc_name: "jane doe".to_string(),
            cc_number: "2222333344445555".to_string(),
            cc_exp_month: 3,
            cc_exp_year: 2022,
            cc_type: "visa".to_string(),
            ..Default::default()
        };
        insert_record(&db, &credit_card)?;

        // create a tombstone record with the same guid
        let tombstone_result = insert_tombstone_record(&db, credit_card.guid.to_string());

        let expected_error_message = "guid exists in `credit_cards_data`";
        assert_eq!(
            expected_error_message,
            tombstone_result.unwrap_err().to_string()
        );

        Ok(())
    }

    #[test]
    fn test_credit_card_touch() -> Result<()> {
        let db = new_mem_db();
        let saved_credit_card = add_credit_card(
            &db,
            UpdatableCreditCardFields {
                cc_name: "john doe".to_string(),
                cc_number: "5555666677778888".to_string(),
                cc_exp_month: 5,
                cc_exp_year: 2024,
                cc_type: "visa".to_string(),
            },
        )?;

        assert_eq!(saved_credit_card.sync_change_counter, 1);
        assert_eq!(saved_credit_card.times_used, 0);

        touch(&db, &saved_credit_card.guid)?;

        let touched_credit_card = get_credit_card(&db, &saved_credit_card.guid)?;

        assert_eq!(touched_credit_card.sync_change_counter, 2);
        assert_eq!(touched_credit_card.times_used, 1);

        Ok(())
    }

    #[test]
    fn test_credit_card_sync_reset() -> Result<()> {
        let mut db = new_mem_db();
        let tx = &db.transaction()?;

        // create a record
        let credit_card = InternalCreditCard {
            guid: Guid::random(),
            cc_name: "jane doe".to_string(),
            cc_number: "2222333344445555".to_string(),
            cc_exp_month: 3,
            cc_exp_year: 2022,
            cc_type: "visa".to_string(),

            ..InternalCreditCard::default()
        };
        insert_record(&tx, &credit_card)?;

        // create a mirror record
        let mirror_record = InternalCreditCard {
            guid: Guid::random(),
            cc_name: "jane doe".to_string(),
            cc_number: "2222333344445555".to_string(),
            cc_exp_month: 3,
            cc_exp_year: 2022,
            cc_type: "visa".to_string(),

            ..InternalCreditCard::default()
        };
        insert_mirror_record(&tx, &mirror_record);

        // create a tombstone record
        let tombstone_guid = Guid::random();
        insert_tombstone_record(&tx, tombstone_guid.to_string())?;

        // create sync metadata
        let global_guid = Guid::new("AAAA");
        let coll_guid = Guid::new("AAAA");
        let ids = CollSyncIds {
            global: global_guid.clone(),
            coll: coll_guid.clone(),
        };
        put_meta(&tx, GLOBAL_SYNCID_META_KEY, &ids.global)?;
        put_meta(&tx, COLLECTION_SYNCID_META_KEY, &ids.coll)?;

        // call reset for sign out
        reset_in_tx(&tx, &EngineSyncAssociation::Disconnected)?;

        // check that sync change counter has been reset
        let reset_record_exists: bool = tx.query_row(
            "SELECT EXISTS (
                SELECT 1
                FROM credit_cards_data
                WHERE sync_change_counter = 1
            )",
            NO_PARAMS,
            |row| row.get(0),
        )?;
        assert!(reset_record_exists);

        // check that the mirror and tombstone tables have no records
        assert!(get_all(&tx, "credit_cards_mirror".to_string())?.is_empty());
        assert!(get_all(&tx, "credit_cards_tombstones".to_string())?.is_empty());

        // check that the last sync time was reset to 0
        let expected_sync_time = 0;
        assert_eq!(
            get_meta::<i64>(&tx, LAST_SYNC_META_KEY)?.unwrap_or(1),
            expected_sync_time
        );

        // check that the meta records were deleted
        assert!(get_meta::<String>(&tx, GLOBAL_SYNCID_META_KEY)?.is_none());
        assert!(get_meta::<String>(&tx, COLLECTION_SYNCID_META_KEY)?.is_none());

        clear_tables(&tx)?;

        // re-populating the tables
        insert_record(&tx, &credit_card)?;
        insert_mirror_record(&tx, &mirror_record);
        insert_tombstone_record(&tx, tombstone_guid.to_string())?;

        // call reset for sign in
        reset_in_tx(&tx, &EngineSyncAssociation::Connected(ids))?;

        // check that the meta records were set
        let retrieved_global_sync_id = get_meta::<String>(&tx, GLOBAL_SYNCID_META_KEY)?;
        assert_eq!(
            retrieved_global_sync_id.unwrap_or_default(),
            global_guid.to_string()
        );

        let retrieved_coll_sync_id = get_meta::<String>(&tx, COLLECTION_SYNCID_META_KEY)?;
        assert_eq!(
            retrieved_coll_sync_id.unwrap_or_default(),
            coll_guid.to_string()
        );

        Ok(())
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::{
    models::credit_card::{InternalCreditCard, UpdatableCreditCardFields},
    schema::{CREDIT_CARD_COMMON_COLS, CREDIT_CARD_COMMON_VALS},
};
use crate::error::*;

use rusqlite::{Connection, Transaction, NO_PARAMS};
use sync_guid::Guid;
use types::Timestamp;

pub(crate) fn add_credit_card(
    conn: &Connection,
    new_credit_card_fields: UpdatableCreditCardFields,
) -> Result<InternalCreditCard> {
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
        metadata: Default::default(),
    };

    let tx = conn.unchecked_transaction()?;
    add_internal_credit_card(&tx, &credit_card)?;
    tx.commit()?;
    Ok(credit_card)
}

pub(crate) fn add_internal_credit_card(
    tx: &Transaction<'_>,
    card: &InternalCreditCard,
) -> Result<()> {
    tx.execute_named(
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
            ":cc_number": card.cc_number,
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

pub(crate) fn get_all_credit_cards(conn: &Connection) -> Result<Vec<InternalCreditCard>> {
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

/// Updates all fields including metadata - although the change counter gets
/// slightly special treatment (eg, when called by Sync we don't want the
/// change counter incremented).
pub(crate) fn update_internal_credit_card(
    tx: &Transaction<'_>,
    card: &InternalCreditCard,
    flag_as_changed: bool,
) -> Result<()> {
    let change_counter_increment = flag_as_changed as u32; // will be 1 or 0
    tx.execute_named(
        "UPDATE credit_cards_data
        SET cc_name                     = :cc_name,
            cc_number                   = :cc_number,
            cc_exp_month                = :cc_exp_month,
            cc_exp_year                 = :cc_exp_year,
            cc_type                     = :cc_type,
            time_created                = :time_created,
            time_last_used              = :time_last_used,
            time_last_modified          = :time_last_modified,
            times_used                  = :times_used,
            sync_change_counter         = sync_change_counter + :change_incr,
        WHERE guid                      = :guid",
        rusqlite::named_params! {
            ":cc_name": card.cc_name,
            ":cc_number": card.cc_number,
            ":cc_exp_month": card.cc_exp_month,
            ":cc_exp_year": card.cc_exp_year,
            ":cc_type": card.cc_type,
            ":time_created": card.metadata.time_created,
            ":time_last_used": card.metadata.time_last_used,
            ":time_last_modified": card.metadata.time_last_modified,
            ":times_used": card.metadata.times_used,
            ":change_incr": change_counter_increment,
            ":guid": card.guid,
        },
    )?;
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::test::new_mem_db;

    pub fn get_all(
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

    pub fn insert_tombstone_record(
        conn: &Connection,
        guid: String,
    ) -> rusqlite::Result<usize, rusqlite::Error> {
        conn.execute_named(
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

    pub(crate) fn insert_mirror_record(conn: &Connection, credit_card: InternalCreditCard) {
        // This should probably be in the sync module, but it's used here.
        let guid = credit_card.guid.clone();
        let payload = credit_card
            .into_payload()
            .expect("is json")
            .into_json_string();
        conn.execute_named(
            "INSERT INTO credit_cards_mirror (guid, payload)
             VALUES (:guid, :payload)",
            rusqlite::named_params! {
                ":guid": guid,
                ":payload": &payload,
            },
        )
        .expect("should insert");
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

        // check that sync_change_counter was set to 0.
        assert_eq!(0, saved_credit_card.metadata.sync_change_counter);

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
        assert_eq!(1, updated_credit_card.metadata.sync_change_counter);

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
        let cc2_guid = saved_credit_card2.guid.clone();
        insert_mirror_record(&db, saved_credit_card2);

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
            &[&cc2_guid],
            |row| row.get(0),
        )?;
        assert!(tombstone_exists);

        // remove the tombstone record
        db.execute_named(
            "DELETE FROM credit_cards_tombstones
            WHERE guid = :guid",
            rusqlite::named_params! {
                ":guid": cc2_guid,
            },
        )?;

        Ok(())
    }

    #[test]
    fn test_credit_card_trigger_on_create() -> Result<()> {
        let db = new_mem_db();
        let tx = db.unchecked_transaction()?;
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
        let db = new_mem_db();
        let tx = db.unchecked_transaction()?;
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

        assert_eq!(saved_credit_card.metadata.sync_change_counter, 0);
        assert_eq!(saved_credit_card.metadata.times_used, 0);

        touch(&db, &saved_credit_card.guid)?;

        let touched_credit_card = get_credit_card(&db, &saved_credit_card.guid)?;

        assert_eq!(touched_credit_card.metadata.sync_change_counter, 1);
        assert_eq!(touched_credit_card.metadata.times_used, 1);

        Ok(())
    }
}

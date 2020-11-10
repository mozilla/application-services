/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::models::credit_card::{CreditCard, InternalCreditCard, NewCreditCardFields};
use crate::db::schema::CREDIT_CARD_COMMON_COLS;
use crate::error::*;

use rusqlite::{Connection, NO_PARAMS};
use sync_guid::Guid;
use types::Timestamp;

pub fn add_credit_card(
    conn: &Connection,
    new_credit_card_fields: NewCreditCardFields,
) -> Result<InternalCreditCard> {
    let tx = conn.unchecked_transaction()?;

    let credit_card = InternalCreditCard {
        guid: Guid::random(),
        fields: new_credit_card_fields,
        time_created: Timestamp::now(),
        time_last_used: Some(Timestamp::now()),
        time_last_modified: Timestamp::now(),
        times_used: 0,
        sync_change_counter: 1,
    };

    tx.execute_named(
        &format!(
            "INSERT OR IGNORE INTO credit_cards_data (
                {common_cols}
            ) VALUES (
                :guid,
                :cc_name,
                :cc_number,
                :cc_exp_month,
                :cc_exp_year,
                :cc_type,
                :time_created,
                :time_last_used,
                :time_last_modified,
                :times_used,
                :sync_change_counter
            )",
            common_cols = CREDIT_CARD_COMMON_COLS
        ),
        rusqlite::named_params! {
            ":guid": credit_card.guid,
            ":cc_name": credit_card.fields.cc_name,
            ":cc_number": credit_card.fields.cc_number,
            ":cc_exp_month": credit_card.fields.cc_exp_month,
            ":cc_exp_year": credit_card.fields.cc_exp_year,
            ":cc_type": credit_card.fields.cc_type,
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

pub fn get_credit_card(conn: &Connection, guid: String) -> Result<InternalCreditCard> {
    let tx = conn.unchecked_transaction()?;
    let sql = format!(
        "SELECT
            {common_cols}
        FROM credit_cards_data
        WHERE guid = :guid",
        common_cols = CREDIT_CARD_COMMON_COLS
    );

    let credit_card = tx.query_row(&sql, &[guid.as_str()], InternalCreditCard::from_row)?;

    tx.commit()?;
    Ok(credit_card)
}

pub fn get_all_credit_cards(conn: &Connection) -> Result<Vec<InternalCreditCard>> {
    let tx = conn.unchecked_transaction()?;
    let credit_cards;
    let sql = format!(
        "SELECT
            {common_cols}
        FROM credit_cards_data",
        common_cols = CREDIT_CARD_COMMON_COLS
    );

    {
        let mut stmt = tx.prepare(&sql)?;
        credit_cards = stmt
            .query_map(NO_PARAMS, InternalCreditCard::from_row)?
            .collect::<Result<Vec<InternalCreditCard>, _>>()?;
    }

    tx.commit()?;
    Ok(credit_cards)
}

pub fn update_credit_card(conn: &Connection, credit_card: &CreditCard) -> Result<()> {
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
            ":guid": credit_card.guid.as_str(),
        },
    )?;

    tx.commit()?;
    Ok(())
}

pub fn delete_credit_card(conn: &Connection, guid: String) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;

    // check that guid exists
    let exists = tx.query_row(
        "SELECT EXISTS (
            SELECT 1
            FROM credit_cards_data d
            WHERE guid = :guid
                AND NOT EXISTS (
                    SELECT 1
                    FROM   credit_cards_tombstones t
                    WHERE  d.guid = t.guid
                )
        )",
        &[guid.as_str()],
        |row| row.get(0),
    )?;

    // check that guid exists in the mirror
    let exists_in_mirror: bool = tx.query_row(
        "SELECT EXISTS (
            SELECT 1
            FROM credit_cards_mirror
            WHERE guid = :guid
        )",
        &[guid.as_str()],
        |row| row.get(0),
    )?;

    if exists {
        tx.execute_named(
            "DELETE FROM credit_cards_data
            WHERE guid = :guid",
            rusqlite::named_params! {
                ":guid": guid.as_str(),
            },
        )?;

        if exists_in_mirror {
            tx.execute_named(
                "INSERT OR IGNORE INTO credit_cards_tombstones (
                    guid,
                    time_deleted
                ) VALUES (
                    :guid,
                    :time_deleted
                )",
                rusqlite::named_params! {
                    ":guid": guid.as_str(),
                    ":time_deleted": Timestamp::now(),
                },
            )?;
        }
    }

    tx.commit()?;
    Ok(exists)
}

pub fn touch(conn: &Connection, guid: String) -> Result<()> {
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
mod tests {
    use super::*;
    use crate::db::test::new_mem_db;

    #[test]
    fn test_credit_card_create_and_read() -> Result<()> {
        let db = new_mem_db();

        let saved_credit_card = add_credit_card(
            &db,
            NewCreditCardFields {
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
        let retrieved_credit_card = get_credit_card(&db, saved_credit_card.guid.to_string())?;

        assert_eq!(saved_credit_card.guid, retrieved_credit_card.guid);
        assert_eq!(
            saved_credit_card.fields.cc_name,
            retrieved_credit_card.fields.cc_name
        );
        assert_eq!(
            saved_credit_card.fields.cc_number,
            retrieved_credit_card.fields.cc_number
        );
        assert_eq!(
            saved_credit_card.fields.cc_exp_month,
            retrieved_credit_card.fields.cc_exp_month
        );
        assert_eq!(
            saved_credit_card.fields.cc_exp_year,
            retrieved_credit_card.fields.cc_exp_year
        );
        assert_eq!(
            saved_credit_card.fields.cc_type,
            retrieved_credit_card.fields.cc_type
        );

        // converting the created record into a tombstone to check that it's not returned on a second `get_credit_card` call
        let delete_result = delete_credit_card(&db, saved_credit_card.guid.to_string());
        assert!(delete_result.is_ok());
        assert!(delete_result?);

        assert!(get_credit_card(&db, saved_credit_card.guid.to_string()).is_err());

        Ok(())
    }

    #[test]
    fn test_credit_card_read_all() -> Result<()> {
        let db = new_mem_db();

        let saved_credit_card = add_credit_card(
            &db,
            NewCreditCardFields {
                cc_name: "jane doe".to_string(),
                cc_number: "2222333344445555".to_string(),
                cc_exp_month: 3,
                cc_exp_year: 2022,
                cc_type: "visa".to_string(),
            },
        )?;

        let saved_credit_card2 = add_credit_card(
            &db,
            NewCreditCardFields {
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
            NewCreditCardFields {
                cc_name: "abraham lincoln".to_string(),
                cc_number: "1111222233334444".to_string(),
                cc_exp_month: 1,
                cc_exp_year: 2024,
                cc_type: "amex".to_string(),
            },
        )?;

        let delete_result = delete_credit_card(&db, saved_credit_card3.guid.to_string());
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
            NewCreditCardFields {
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
            &CreditCard {
                guid: saved_credit_card.guid.to_string(),
                cc_name: expected_cc_name.clone(),
                cc_number: "1111222233334444".to_string(),
                cc_type: "mastercard".to_string(),
                cc_exp_month: 10,
                cc_exp_year: 2025,
            },
        );
        assert!(update_result.is_ok());

        let updated_credit_card = get_credit_card(&db, saved_credit_card.guid.to_string())?;

        assert_eq!(saved_credit_card.guid, updated_credit_card.guid);
        assert_eq!(expected_cc_name, updated_credit_card.fields.cc_name);

        //check that the sync_change_counter was incremented
        assert_eq!(2, updated_credit_card.sync_change_counter);

        Ok(())
    }

    #[test]
    fn test_credit_card_delete() -> Result<()> {
        let db = new_mem_db();

        let saved_credit_card = add_credit_card(
            &db,
            NewCreditCardFields {
                cc_name: "john deer".to_string(),
                cc_number: "1111222233334444".to_string(),
                cc_exp_month: 10,
                cc_exp_year: 2025,
                cc_type: "mastercard".to_string(),
            },
        )?;

        let delete_result = delete_credit_card(&db, saved_credit_card.guid.to_string());
        assert!(delete_result.is_ok());
        assert!(delete_result?);

        let saved_credit_card2 = add_credit_card(
            &db,
            NewCreditCardFields {
                cc_name: "john doe".to_string(),
                cc_number: "5555666677778888".to_string(),
                cc_exp_month: 5,
                cc_exp_year: 2024,
                cc_type: "visa".to_string(),
            },
        )?;

        // create a mirror record to check that a tombstone record is created upon deletion
        db.execute_named(
            "INSERT OR IGNORE INTO credit_cards_mirror (
                guid,
                cc_name,
                cc_number,
                cc_exp_month,
                cc_exp_year,
                cc_type,
                time_created,
                time_last_used,
                time_last_modified,
                times_used
            ) VALUES (
                :guid,
                :cc_name,
                :cc_number,
                :cc_exp_month,
                :cc_exp_year,
                :cc_type,
                :time_created,
                :time_last_used,
                :time_last_modified,
                :times_used
            )",
            rusqlite::named_params! {
                ":guid": saved_credit_card2.guid,
                ":cc_name": saved_credit_card2.fields.cc_name,
                ":cc_number": saved_credit_card2.fields.cc_number,
                ":cc_exp_month": saved_credit_card2.fields.cc_exp_month,
                ":cc_exp_year": saved_credit_card2.fields.cc_exp_year,
                ":cc_type": saved_credit_card2.fields.cc_type,
                ":time_created": saved_credit_card2.time_created,
                ":time_last_used": saved_credit_card2.time_last_used,
                ":time_last_modified": saved_credit_card2.time_last_modified,
                ":times_used": saved_credit_card2.times_used,
            },
        )?;

        let delete_result2 = delete_credit_card(&db, saved_credit_card2.guid.to_string());
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
                ":guid": saved_credit_card2.guid.as_str(),
            },
        )?;

        Ok(())
    }

    #[test]
    fn test_credit_card_trigger_on_create() {
        let db = new_mem_db();
        let guid = Guid::random();

        // create a tombstone record
        let tombstone_result = db.execute_named(
            "INSERT OR IGNORE INTO credit_cards_tombstones (
                guid,
                time_deleted
            ) VALUES (
                :guid,
                :time_deleted
            )",
            rusqlite::named_params! {
                ":guid": guid.as_str(),
                ":time_deleted": Timestamp::now(),
            },
        );
        assert!(tombstone_result.is_ok());

        // create a new credit card with the tombstone's guid
        let credit_card = InternalCreditCard {
            guid,
            fields: NewCreditCardFields {
                cc_name: "john deer".to_string(),
                cc_number: "1111222233334444".to_string(),
                cc_exp_month: 10,
                cc_exp_year: 2025,
                cc_type: "mastercard".to_string(),
            },

            ..InternalCreditCard::default()
        };

        let add_credit_card_result = db.execute_named(
            &format!(
                "INSERT OR IGNORE INTO credit_cards_data (
                    {common_cols}
                ) VALUES (
                    :guid,
                    :cc_name,
                    :cc_number,
                    :cc_exp_month,
                    :cc_exp_year,
                    :cc_type,
                    :time_created,
                    :time_last_used,
                    :time_last_modified,
                    :times_used,
                    :sync_change_counter
                )",
                common_cols = CREDIT_CARD_COMMON_COLS
            ),
            rusqlite::named_params! {
                ":guid": credit_card.guid,
                ":cc_name": credit_card.fields.cc_name,
                ":cc_number": credit_card.fields.cc_number,
                ":cc_exp_month": credit_card.fields.cc_exp_month,
                ":cc_exp_year": credit_card.fields.cc_exp_year,
                ":cc_type": credit_card.fields.cc_type,
                ":time_created": credit_card.time_created,
                ":time_last_used": credit_card.time_last_used,
                ":time_last_modified": credit_card.time_last_modified,
                ":times_used": credit_card.times_used,
                ":sync_change_counter": credit_card.sync_change_counter,
            },
        );
        assert!(add_credit_card_result.is_err());

        let expected_error_message = "guid exists in `credit_cards_tombstones`";
        assert_eq!(
            expected_error_message,
            add_credit_card_result.unwrap_err().to_string()
        );
    }

    #[test]
    fn test_credit_card_trigger_on_delete() {
        let db = new_mem_db();
        let guid = Guid::random();

        // create an credit card
        let credit_card = InternalCreditCard {
            guid,
            fields: NewCreditCardFields {
                cc_name: "jane doe".to_string(),
                cc_number: "2222333344445555".to_string(),
                cc_exp_month: 3,
                cc_exp_year: 2022,
                cc_type: "visa".to_string(),
            },

            ..InternalCreditCard::default()
        };

        let add_credit_card_result = db.execute_named(
            &format!(
                "INSERT OR IGNORE INTO credit_cards_data (
                    {common_cols}
                ) VALUES (
                    :guid,
                    :cc_name,
                    :cc_number,
                    :cc_exp_month,
                    :cc_exp_year,
                    :cc_type,
                    :time_created,
                    :time_last_used,
                    :time_last_modified,
                    :times_used,
                    :sync_change_counter
                )",
                common_cols = CREDIT_CARD_COMMON_COLS
            ),
            rusqlite::named_params! {
                ":guid": credit_card.guid,
                ":cc_name": credit_card.fields.cc_name,
                ":cc_number": credit_card.fields.cc_number,
                ":cc_exp_month": credit_card.fields.cc_exp_month,
                ":cc_exp_year": credit_card.fields.cc_exp_year,
                ":cc_type": credit_card.fields.cc_type,
                ":time_created": credit_card.time_created,
                ":time_last_used": credit_card.time_last_used,
                ":time_last_modified": credit_card.time_last_modified,
                ":times_used": credit_card.times_used,
                ":sync_change_counter": credit_card.sync_change_counter,
            },
        );
        assert!(add_credit_card_result.is_ok());

        // create a tombstone record with the same guid
        let tombstone_result = db.execute_named(
            "INSERT OR IGNORE INTO credit_cards_tombstones (
                guid,
                time_deleted
            ) VALUES (
                :guid,
                :time_deleted
            )",
            rusqlite::named_params! {
                ":guid": credit_card.guid.as_str(),
                ":time_deleted": Timestamp::now(),
            },
        );
        assert!(tombstone_result.is_err());

        let expected_error_message = "guid exists in `credit_cards_data`";
        assert_eq!(
            expected_error_message,
            tombstone_result.unwrap_err().to_string()
        );
    }

    #[test]
    fn test_credit_card_touch() -> Result<()> {
        let db = new_mem_db();
        let saved_credit_card = add_credit_card(
            &db,
            NewCreditCardFields {
                cc_name: "john doe".to_string(),
                cc_number: "5555666677778888".to_string(),
                cc_exp_month: 5,
                cc_exp_year: 2024,
                cc_type: "visa".to_string(),
            },
        )?;

        assert_eq!(saved_credit_card.sync_change_counter, 1);
        assert_eq!(saved_credit_card.times_used, 0);

        touch(&db, saved_credit_card.guid.to_string())?;

        let touched_credit_card = get_credit_card(&db, saved_credit_card.guid.to_string())?;

        assert_eq!(touched_credit_card.sync_change_counter, 2);
        assert_eq!(touched_credit_card.times_used, 1);

        Ok(())
    }
}

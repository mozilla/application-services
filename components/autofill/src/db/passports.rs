/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use crate::db::{
    models::{
        passport::{InternalPassport, UpdatablePassportFields},
        Metadata,
    },
    schema::{PASSPORT_COMMON_COLS, PASSPORT_COMMON_VALS},
};
use crate::error::*;

use rusqlite::{Connection, Transaction};
use sync_guid::Guid;
use types::Timestamp;

pub(crate) fn add_passport(
    conn: &Connection,
    new: UpdatablePassportFields,
) -> Result<InternalPassport> {
    let tx = conn.unchecked_transaction()?;
    let now = Timestamp::now();

    let passport = InternalPassport {
        guid: Guid::random(),
        name: new.name,
        country: new.country,
        passport_number: new.passport_number,
        issue_date_month: new.issue_date_month,
        issue_date_day: new.issue_date_day,
        issue_date_year: new.issue_date_year,
        expiry_date_month: new.expiry_date_month,
        expiry_date_day: new.expiry_date_day,
        expiry_date_year: new.expiry_date_year,
        metadata: Metadata {
            time_created: now,
            time_last_modified: now,
            ..Default::default()
        },
    };
    add_internal_passport(&tx, &passport)?;
    tx.commit()?;
    Ok(passport)
}

fn add_internal_passport(tx: &Transaction<'_>, passport: &InternalPassport) -> Result<()> {
    tx.execute(
        &format!(
            "INSERT INTO passports_data (
                {common_cols},
                sync_change_counter
            ) VALUES (
                {common_vals},
                :sync_change_counter
            )",
            common_cols = PASSPORT_COMMON_COLS,
            common_vals = PASSPORT_COMMON_VALS,
        ),
        rusqlite::named_params! {
            ":guid": passport.guid,
            ":name": passport.name,
            ":country": passport.country,
            ":passport_number": passport.passport_number,
            ":issue_date_month": passport.issue_date_month,
            ":issue_date_day": passport.issue_date_day,
            ":issue_date_year": passport.issue_date_year,
            ":expiry_date_month": passport.expiry_date_month,
            ":expiry_date_day": passport.expiry_date_day,
            ":expiry_date_year": passport.expiry_date_year,
            ":time_created": passport.metadata.time_created,
            ":time_last_used": passport.metadata.time_last_used,
            ":time_last_modified": passport.metadata.time_last_modified,
            ":times_used": passport.metadata.times_used,
            ":sync_change_counter": passport.metadata.sync_change_counter,
        },
    )?;
    Ok(())
}

pub(crate) fn get_passport(conn: &Connection, guid: &Guid) -> Result<InternalPassport> {
    let sql = format!(
        "SELECT
            {common_cols},
            sync_change_counter
        FROM passports_data
        WHERE guid = :guid",
        common_cols = PASSPORT_COMMON_COLS
    );
    conn.query_row(&sql, [guid], InternalPassport::from_row)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NoSuchRecord(guid.to_string()),
            e => e.into(),
        })
}

pub(crate) fn get_all_passports(conn: &Connection) -> Result<Vec<InternalPassport>> {
    let sql = format!(
        "SELECT
            {common_cols},
            sync_change_counter
        FROM passports_data",
        common_cols = PASSPORT_COMMON_COLS
    );
    let mut stmt = conn.prepare(&sql)?;
    let passports = stmt
        .query_map([], InternalPassport::from_row)?
        .collect::<std::result::Result<Vec<InternalPassport>, _>>()?;
    Ok(passports)
}

pub(crate) fn count_all_passports(conn: &Connection) -> Result<i64> {
    let sql = "SELECT COUNT(*) FROM passports_data";
    let mut stmt = conn.prepare(sql)?;
    let count: i64 = stmt.query_row([], |row| row.get(0))?;
    Ok(count)
}

/// Updates just the "updatable" columns - suitable for exposure as a public
/// API.
pub(crate) fn update_passport(
    conn: &Connection,
    guid: &Guid,
    passport: &UpdatablePassportFields,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE passports_data
        SET name               = :name,
            country            = :country,
            passport_number    = :passport_number,
            issue_date_month   = :issue_date_month,
            issue_date_day     = :issue_date_day,
            issue_date_year    = :issue_date_year,
            expiry_date_month  = :expiry_date_month,
            expiry_date_day    = :expiry_date_day,
            expiry_date_year   = :expiry_date_year,
            time_last_modified = :time_last_modified,
            sync_change_counter = sync_change_counter + 1
        WHERE guid             = :guid",
        rusqlite::named_params! {
            ":name": passport.name,
            ":country": passport.country,
            ":passport_number": passport.passport_number,
            ":issue_date_month": passport.issue_date_month,
            ":issue_date_day": passport.issue_date_day,
            ":issue_date_year": passport.issue_date_year,
            ":expiry_date_month": passport.expiry_date_month,
            ":expiry_date_day": passport.expiry_date_day,
            ":expiry_date_year": passport.expiry_date_year,
            ":time_last_modified": Timestamp::now(),
            ":guid": guid,
        },
    )?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn delete_passport(conn: &Connection, guid: &Guid) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;
    // execute returns how many rows were affected.
    let exists = tx.execute(
        "DELETE FROM passports_data WHERE guid = :guid",
        rusqlite::named_params! { ":guid": guid },
    )? != 0;
    tx.commit()?;
    Ok(exists)
}

pub fn touch(conn: &Connection, guid: &Guid) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let now_ms = Timestamp::now();
    tx.execute(
        "UPDATE passports_data
        SET time_last_used = :time_last_used,
            times_used     = times_used + 1,
            sync_change_counter = sync_change_counter + 1
        WHERE guid         = :guid",
        rusqlite::named_params! {
            ":time_last_used": now_ms,
            ":guid": guid,
        },
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test::new_mem_db;

    fn sample_fields(name: &str, number: &str) -> UpdatablePassportFields {
        UpdatablePassportFields {
            name: name.to_string(),
            country: "CA".to_string(),
            passport_number: number.to_string(),
            issue_date_month: 1,
            issue_date_day: 15,
            issue_date_year: 2020,
            expiry_date_month: 1,
            expiry_date_day: 15,
            expiry_date_year: 2030,
        }
    }

    #[test]
    fn test_passport_create_and_read() -> Result<()> {
        let db = new_mem_db();

        let saved = add_passport(&db, sample_fields("Jane Doe", "X1234567"))?;

        // add populated the guid and timestamps
        assert_ne!(Guid::default(), saved.guid);
        assert_ne!(0, saved.metadata.time_created.as_millis());
        assert_ne!(0, saved.metadata.time_last_modified.as_millis());

        let retrieved = get_passport(&db, &saved.guid)?;
        assert_eq!(saved.guid, retrieved.guid);
        assert_eq!(retrieved.name, "Jane Doe");
        assert_eq!(retrieved.country, "CA");
        assert_eq!(retrieved.passport_number, "X1234567");
        assert_eq!(retrieved.issue_date_month, 1);
        assert_eq!(retrieved.issue_date_day, 15);
        assert_eq!(retrieved.issue_date_year, 2020);
        assert_eq!(retrieved.expiry_date_year, 2030);

        // deleting removes it
        assert!(delete_passport(&db, &saved.guid)?);
        assert!(get_passport(&db, &saved.guid).is_err());

        Ok(())
    }

    #[test]
    fn test_passport_missing_guid() {
        let db = new_mem_db();
        let guid = Guid::random();
        let result = get_passport(&db, &guid);
        assert_eq!(
            result.unwrap_err().to_string(),
            Error::NoSuchRecord(guid.to_string()).to_string()
        );
    }

    #[test]
    fn test_passport_read_all() -> Result<()> {
        let db = new_mem_db();

        let a = add_passport(&db, sample_fields("Jane Doe", "A1"))?;
        let b = add_passport(&db, sample_fields("John Deer", "B2"))?;
        let c = add_passport(&db, sample_fields("Abe Lincoln", "C3"))?;

        assert!(delete_passport(&db, &c.guid)?);

        let all = get_all_passports(&db)?;
        assert_eq!(all.len(), 2);
        assert_eq!(count_all_passports(&db)?, 2);

        let guids = [all[0].guid.as_str(), all[1].guid.as_str()];
        assert!(guids.contains(&a.guid.as_str()));
        assert!(guids.contains(&b.guid.as_str()));

        Ok(())
    }

    #[test]
    fn test_passport_update() -> Result<()> {
        let db = new_mem_db();
        let saved = add_passport(&db, sample_fields("John Deer", "Z9"))?;
        assert_eq!(saved.metadata.sync_change_counter, 0);

        let mut fields = sample_fields("John Doe", "Z9");
        fields.expiry_date_year = 2035;
        update_passport(&db, &saved.guid, &fields)?;

        let updated = get_passport(&db, &saved.guid)?;
        assert_eq!(updated.name, "John Doe");
        assert_eq!(updated.expiry_date_year, 2035);
        // updating bumps the sync change counter
        assert_eq!(updated.metadata.sync_change_counter, 1);

        Ok(())
    }

    #[test]
    fn test_passport_delete() -> Result<()> {
        let db = new_mem_db();
        let saved = add_passport(&db, sample_fields("Jane Doe", "D1"))?;

        assert!(delete_passport(&db, &saved.guid)?);
        // deleting a non-existent record returns false
        assert!(!delete_passport(&db, &saved.guid)?);

        Ok(())
    }

    #[test]
    fn test_passport_touch() -> Result<()> {
        let db = new_mem_db();
        let saved = add_passport(&db, sample_fields("Jane Doe", "T1"))?;
        assert_eq!(saved.metadata.times_used, 0);
        assert_eq!(saved.metadata.sync_change_counter, 0);

        touch(&db, &saved.guid)?;

        let touched = get_passport(&db, &saved.guid)?;
        assert_eq!(touched.metadata.times_used, 1);
        assert!(touched.metadata.time_last_used.as_millis() > 0);
        // touching bumps the sync change counter
        assert_eq!(touched.metadata.sync_change_counter, 1);

        Ok(())
    }
}

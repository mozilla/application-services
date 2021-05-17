/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::Result;
use rusqlite::{Connection, NO_PARAMS};

pub const ADDRESS_COMMON_COLS: &str = "
    guid,
    given_name,
    additional_name,
    family_name,
    organization,
    street_address,
    address_level3,
    address_level2,
    address_level1,
    postal_code,
    country,
    tel,
    email,
    time_created,
    time_last_used,
    time_last_modified,
    times_used";

pub const ADDRESS_COMMON_VALS: &str = "
    :guid,
    :given_name,
    :additional_name,
    :family_name,
    :organization,
    :street_address,
    :address_level3,
    :address_level2,
    :address_level1,
    :postal_code,
    :country,
    :tel,
    :email,
    :time_created,
    :time_last_used,
    :time_last_modified,
    :times_used";

pub const CREDIT_CARD_COMMON_COLS: &str = "
    guid,
    cc_name,
    cc_number_enc,
    cc_number_last_4,
    cc_exp_month,
    cc_exp_year,
    cc_type,
    time_created,
    time_last_used,
    time_last_modified,
    times_used";

pub const CREDIT_CARD_COMMON_VALS: &str = "
    :guid,
    :cc_name,
    :cc_number_enc,
    :cc_number_last_4,
    :cc_exp_month,
    :cc_exp_year,
    :cc_type,
    :time_created,
    :time_last_used,
    :time_last_modified,
    :times_used";

const CREATE_SHARED_SCHEMA_SQL: &str = include_str!("../../sql/create_shared_schema.sql");
const CREATE_SHARED_TRIGGERS_SQL: &str = include_str!("../../sql/create_shared_triggers.sql");
const CREATE_SYNC_TEMP_TABLES_SQL: &str = include_str!("../../sql/create_sync_temp_tables.sql");

// The schema version - changes to this typically require a custom
// migration code.
const VERSION: i64 = 2;

fn get_current_schema_version(db: &Connection) -> Result<i64> {
    Ok(db.query_row_and_then("PRAGMA user_version", NO_PARAMS, |row| row.get(0))?)
}

pub fn init(db: &Connection) -> Result<()> {
    let version = get_current_schema_version(db)?;
    if version != VERSION {
        if version < VERSION {
            upgrade(db, version)?;
        } else {
            log::warn!(
                "Optimistically loaded future schema version {} (we only understand version {})",
                version,
                VERSION
            );
            // Downgrade the schema version, so that anything added with our
            // schema is migrated forward when the newer library reads our
            // database.
            db.execute_batch(&format!("PRAGMA user_version = {};", VERSION))?;
        }
        create(db)?;
    }
    Ok(())
}

fn create(db: &Connection) -> Result<()> {
    log::debug!("Creating schema");
    db.execute_batch(
        format!(
            "{}\n{}",
            CREATE_SHARED_SCHEMA_SQL, CREATE_SHARED_TRIGGERS_SQL
        )
        .as_str(),
    )?;
    db.execute(
        &format!("PRAGMA user_version = {version}", version = VERSION),
        NO_PARAMS,
    )?;
    Ok(())
}

pub fn create_empty_sync_temp_tables(db: &Connection) -> Result<()> {
    log::debug!("Initializing sync temp tables");
    db.execute_batch(CREATE_SYNC_TEMP_TABLES_SQL)?;
    Ok(())
}

fn upgrade(db: &Connection, from: i64) -> Result<()> {
    log::debug!("Upgrading schema from {} to {}", from, VERSION);
    if from == VERSION {
        return Ok(());
    }
    // Places has a cute `migration` helper we can consider using if we get
    // a few complicated updates, but let's KISS for now.
    if from == 0 {
        // This is a bit painful - there are (probably 3) databases out there
        // that have a schema of 0 but actually exist - ie, we can't assume
        // a schema of zero implies "new database".
        // These databases have a `cc_number` but we need them to have a
        // `cc_number_enc` and `cc_number_last_4`.
        // This was so very early in the Fenix nightly cycle, and before any
        // real UI existed to create cards, so we don't bother trying to
        // migrate them, we just drop the table so it's re-created with the
        // correct schema.
        db.execute("DROP TABLE IF EXISTS credit_cards_data", NO_PARAMS)?;
    } else if from == 1 {
        // Alter cc_number_enc using the 12-step generalized procedure described here:
        // https://sqlite.org/lang_altertable.html
        // Note that all our triggers are TEMP triggers so do not exist when
        // this is called (except possibly by tests which do things like
        // downgrade the version after they are created etc.)
        db.execute_batch(
            "
            CREATE TABLE new_credit_cards_data (
                guid                TEXT NOT NULL PRIMARY KEY CHECK(length(guid) != 0),
                cc_name             TEXT NOT NULL,
                cc_number_enc       TEXT NOT NULL CHECK(length(cc_number_enc) > 20 OR cc_number_enc == ''),
                cc_number_last_4    TEXT NOT NULL CHECK(length(cc_number_last_4) <= 4),
                cc_exp_month        INTEGER,
                cc_exp_year         INTEGER,
                cc_type             TEXT NOT NULL,
                time_created        INTEGER NOT NULL,
                time_last_used      INTEGER,
                time_last_modified  INTEGER NOT NULL,
                times_used          INTEGER NOT NULL,
                sync_change_counter INTEGER NOT NULL
            );
            INSERT INTO new_credit_cards_data(guid, cc_name, cc_number_enc, cc_number_last_4, cc_exp_month,
            cc_exp_year, cc_type, time_created, time_last_used, time_last_modified, times_used,
            sync_change_counter)
            SELECT guid, cc_name, cc_number_enc, cc_number_last_4, cc_exp_month, cc_exp_year, cc_type,
                time_created, time_last_used, time_last_modified, times_used, sync_change_counter
            FROM credit_cards_data;
            DROP TABLE credit_cards_data;
            ALTER TABLE new_credit_cards_data RENAME to credit_cards_data;
            ")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test::new_mem_db;
    use rusqlite::Row;
    use types::Timestamp;

    // Define some structs to handle data in the DB during an upgrade.  This allows us to change
    // the schema without having to update the test code.
    #[derive(Debug, Default)]
    struct CreditCardRowV1 {
        guid: String,
        cc_name: String,
        cc_number_enc: String,
        cc_number_last_4: String,
        cc_type: String,
        cc_exp_month: i64,
        cc_exp_year: i64,
        time_created: Timestamp,
        time_last_used: Timestamp,
        time_last_modified: Timestamp,
        times_used: i64,
        sync_change_counter: i64,
    }

    impl CreditCardRowV1 {
        fn from_row(row: &Row<'_>) -> rusqlite::Result<CreditCardRowV1> {
            Ok(Self {
                guid: row.get("guid")?,
                cc_name: row.get("cc_name")?,
                cc_number_enc: row.get("cc_number_enc")?,
                cc_number_last_4: row.get("cc_number_last_4")?,
                cc_exp_month: row.get("cc_exp_month")?,
                cc_exp_year: row.get("cc_exp_year")?,
                cc_type: row.get("cc_type")?,
                time_created: row.get("time_created")?,
                time_last_used: row.get("time_last_used")?,
                time_last_modified: row.get("time_last_modified")?,
                times_used: row.get("times_used")?,
                sync_change_counter: row.get("sync_change_counter")?,
            })
        }
    }

    #[test]
    fn test_create_schema_twice() {
        let db = new_mem_db();
        db.execute_batch(CREATE_SHARED_SCHEMA_SQL)
            .expect("should allow running main schema creation twice");
        // sync tables aren't created by default, so do it twice here.
        db.execute_batch(CREATE_SYNC_TEMP_TABLES_SQL)
            .expect("should allow running sync temp tables first time");
        db.execute_batch(CREATE_SYNC_TEMP_TABLES_SQL)
            .expect("should allow running sync temp tables second time");
    }

    #[test]
    fn test_upgrade_version_0() {
        let db = new_mem_db();
        // Manually hack things back to where we had to migrate from
        // version 0.
        // We don't care what the old table actually has because we drop it
        // without a migration.
        db.execute_batch(
            "
            DROP TABLE credit_cards_data;
            CREATE TABLE credit_cards_data (guid TEXT NOT NULL PRIMARY KEY);
            PRAGMA user_version = 0;",
        )
        .expect("should work");

        // Just to test what we think we are testing, select a field that
        // doesn't exist now but will after we recreate the table.
        let select_name = "SELECT cc_name from credit_cards_data";
        db.execute_batch(select_name)
            .expect_err("select should fail due to bad field name");
        init(&db).expect("re-init should work");
        // should have dropped and recreated the table, so this select should work.
        db.execute_batch(select_name)
            .expect("select should now work");
    }

    #[test]
    fn test_upgrade_version_1() -> Result<()> {
        let db = new_mem_db();
        // Go back to version 0 of the credit_cards_data table and insert a row
        db.execute_batch(
            "
            -- This trigger exists because the DB is fully initialized - but
            -- in the normal case, we run the upgrade code before we've created
            -- the temp triggers etc.
            DROP TRIGGER credit_cards_tombstones_afterinsert_trigger;
            DROP TABLE credit_cards_data;
            CREATE TABLE credit_cards_data (
                guid                TEXT NOT NULL PRIMARY KEY CHECK(length(guid) != 0),
                cc_name             TEXT NOT NULL,
                cc_number_enc       TEXT NOT NULL CHECK(length(cc_number_enc) > 20),
                cc_number_last_4    TEXT NOT NULL CHECK(length(cc_number_last_4) <= 4),
                cc_exp_month        INTEGER,
                cc_exp_year         INTEGER,
                cc_type             TEXT NOT NULL,
                time_created        INTEGER NOT NULL,
                time_last_used      INTEGER,
                time_last_modified  INTEGER NOT NULL,
                times_used          INTEGER NOT NULL,
                sync_change_counter INTEGER NOT NULL
            );
            INSERT INTO credit_cards_data(guid, cc_name, cc_number_enc, cc_number_last_4, cc_exp_month,
            cc_exp_year, cc_type, time_created, time_last_used, time_last_modified, times_used, sync_change_counter)
            VALUES ('A', 'Jane Doe', '012345678901234567890', '1234', 1, 2020, 'visa', 0, 1, 2, 3, 0);
        ")?;
        // Do the upgrade
        upgrade(&db, 1)?;
        // Check that the old data is still present
        let query_sql = "
            SELECT guid, cc_name, cc_number_enc, cc_number_last_4, cc_exp_month, cc_exp_year, cc_type, time_created,
                time_last_modified, time_last_used, times_used, sync_change_counter
            FROM credit_cards_data
            WHERE guid='A'";
        let credit_card = db.query_row(query_sql, NO_PARAMS, CreditCardRowV1::from_row)?;
        assert_eq!(credit_card.cc_name, "Jane Doe");
        assert_eq!(credit_card.cc_number_enc, "012345678901234567890");
        assert_eq!(credit_card.cc_number_last_4, "1234");
        assert_eq!(credit_card.cc_exp_month, 1);
        assert_eq!(credit_card.cc_exp_year, 2020);
        assert_eq!(credit_card.cc_type, "visa");
        assert_eq!(credit_card.time_created, Timestamp(0));
        assert_eq!(credit_card.time_last_used, Timestamp(1));
        assert_eq!(credit_card.time_last_modified, Timestamp(2));
        assert_eq!(credit_card.times_used, 3);
        assert_eq!(credit_card.sync_change_counter, 0);

        // Test the upgraded check constraint
        db.execute("UPDATE credit_cards_data SET cc_number_enc=''", NO_PARAMS)
            .expect("blank cc_number_enc should be valid");
        db.execute("UPDATE credit_cards_data SET cc_number_enc='x'", NO_PARAMS)
            .expect_err("cc_number_enc should be invalid");

        Ok(())
    }
}

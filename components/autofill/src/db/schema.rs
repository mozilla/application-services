/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::sql_fns;
use rusqlite::functions::FunctionFlags;
use rusqlite::Connection;
use sql_support::open_database::{ErrorHandling, MigrationLogic, Result};

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

pub(super) fn migration_logic() -> MigrationLogic {
    MigrationLogic {
        start_version: 0,
        end_version: 2,
        prepare: Some(prepare),
        init,
        upgrades: vec![upgrade_to_v1, upgrade_to_v2],
        finish: Some(finish),
        error_handling: ErrorHandling::DeleteAndRecreate,
    }
}

fn prepare(conn: &Connection) -> Result<()> {
    define_functions(&conn)?;
    conn.set_prepared_statement_cache_capacity(128);
    Ok(())
}

fn define_functions(c: &Connection) -> Result<()> {
    c.create_scalar_function(
        "generate_guid",
        0,
        FunctionFlags::SQLITE_UTF8,
        sql_fns::generate_guid,
    )?;
    c.create_scalar_function("now", 0, FunctionFlags::SQLITE_UTF8, sql_fns::now)?;

    Ok(())
}

fn init(db: &Connection) -> Result<()> {
    Ok(db.execute_batch(CREATE_SHARED_SCHEMA_SQL)?)
}

fn upgrade_to_v1(db: &Connection) -> Result<()> {
    // This is a bit painful - there are (probably 3) databases out there
    // that have a schema of 0.
    // These databases have a `cc_number` but we need them to have a
    // `cc_number_enc` and `cc_number_last_4`.
    // This was so very early in the Fenix nightly cycle, and before any
    // real UI existed to create cards, so we don't bother trying to
    // migrate them, we just drop the table and re-create it with the
    // correct schema.
    db.execute_batch(
        "
        DROP TABLE IF EXISTS credit_cards_data;
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
        ",
    )?;
    Ok(())
}

fn upgrade_to_v2(db: &Connection) -> Result<()> {
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
    Ok(())
}

fn finish(db: &Connection) -> Result<()> {
    Ok(db.execute_batch(CREATE_SHARED_TRIGGERS_SQL)?)
}

pub fn create_empty_sync_temp_tables(db: &Connection) -> Result<()> {
    log::debug!("Initializing sync temp tables");
    db.execute_batch(CREATE_SYNC_TEMP_TABLES_SQL)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test::new_mem_db;
    use rusqlite::{Row, NO_PARAMS};
    use sql_support::open_database::test_utils::MigratedDatabaseFile;
    use types::Timestamp;

    const CREATE_SHARED_SCHEMA_V0_SQL: &str = include_str!("../../sql/create_shared_schema_v0.sql");
    fn init_v0(db: &Connection) -> Result<()> {
        Ok(db.execute_batch(CREATE_SHARED_SCHEMA_V0_SQL)?)
    }

    // Define some structs to handle data in the DB during an upgrade.  This allows us to change
    // the schema without having to update the test code.
    #[derive(Debug, Default, PartialEq)]
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
        fn from_row(row: &Row<'_>) -> Self {
            Self {
                guid: row.get("guid").unwrap(),
                cc_name: row.get("cc_name").unwrap(),
                cc_number_enc: row.get("cc_number_enc").unwrap(),
                cc_number_last_4: row.get("cc_number_last_4").unwrap(),
                cc_exp_month: row.get("cc_exp_month").unwrap(),
                cc_exp_year: row.get("cc_exp_year").unwrap(),
                cc_type: row.get("cc_type").unwrap(),
                time_created: row.get("time_created").unwrap(),
                time_last_used: row.get("time_last_used").unwrap(),
                time_last_modified: row.get("time_last_modified").unwrap(),
                times_used: row.get("times_used").unwrap(),
                sync_change_counter: row.get("sync_change_counter").unwrap(),
            }
        }

        fn fetch(db: &Connection, guid: &str) -> Self {
            db.query_row(
                "
                SELECT guid, cc_name, cc_number_enc, cc_number_last_4, cc_exp_month, cc_exp_year, cc_type, time_created,
                    time_last_modified, time_last_used, times_used, sync_change_counter
                FROM credit_cards_data
                WHERE guid=?
                ",
                &[guid],
                |r| Ok(Self::from_row(r))).unwrap()
        }

        fn insert(&self, conn: &Connection) {
            conn.execute_named(
                "
                INSERT INTO credit_cards_data(guid, cc_name, cc_number_enc, cc_number_last_4,
                cc_exp_month, cc_exp_year, cc_type, time_created, time_last_used,
                time_last_modified, times_used, sync_change_counter)
                VALUES (:guid, :cc_name, :cc_number_enc, :cc_number_last_4, :cc_exp_month,
                :cc_exp_year, :cc_type, :time_created, :time_last_used, :time_last_modified,
                :times_used, :sync_change_counter)
                ",
                rusqlite::named_params! {
                    ":guid": self.guid,
                    ":cc_name": self.cc_name,
                    ":cc_number_enc": self.cc_number_enc,
                    ":cc_number_last_4": self.cc_number_last_4,
                    ":cc_exp_month": self.cc_exp_month,
                    ":cc_exp_year": self.cc_exp_year,
                    ":cc_type": self.cc_type,
                    ":time_created": self.time_created,
                    ":time_last_used": self.time_last_used,
                    ":time_last_modified": self.time_last_modified,
                    ":times_used": self.times_used,
                    ":sync_change_counter": self.sync_change_counter,
                },
            )
            .unwrap();
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
    fn test_all_upgrades() {
        // Quickly check that all migrations run
        let db_file = MigratedDatabaseFile::new(migration_logic(), init_v0, 0);
        db_file.run_all_upgrades();
    }

    #[test]
    fn test_upgrade_version_0() {
        let db_file = MigratedDatabaseFile::new(migration_logic(), init_v0, 0);
        // Just to test what we think we are testing, select a field that
        // doesn't exist now but will after we recreate the table.
        let select_cc_number_enc = "SELECT cc_number_enc from credit_cards_data";
        db_file
            .open()
            .execute_batch(select_cc_number_enc)
            .expect_err("select should fail due to bad field name");

        db_file.upgrade_to(1);

        db_file
            .open()
            .execute_batch(select_cc_number_enc)
            .expect("select should now work");
    }

    #[test]
    fn test_upgrade_version_1() {
        let db_file = MigratedDatabaseFile::new(migration_logic(), init_v0, 0);
        db_file.upgrade_to(1);

        let orig_row = CreditCardRowV1 {
            guid: "A".to_string(),
            cc_name: "Jane Doe".to_string(),
            cc_number_enc: "012345678901234567890".to_string(),
            cc_number_last_4: "1234".to_string(),
            cc_exp_month: 1,
            cc_exp_year: 2020,
            cc_type: "visa".to_string(),
            time_created: Timestamp(0),
            time_last_used: Timestamp(1),
            time_last_modified: Timestamp(2),
            times_used: 3,
            sync_change_counter: 0,
        };
        orig_row.insert(&db_file.open());
        db_file.upgrade_to(2);
        let db = db_file.open();
        let new_row = CreditCardRowV1::fetch(&db, "A");
        assert_eq!(new_row, orig_row);

        // Test the upgraded check constraint
        db.execute("UPDATE credit_cards_data SET cc_number_enc=''", NO_PARAMS)
            .expect("blank cc_number_enc should be valid");
        db.execute("UPDATE credit_cards_data SET cc_number_enc='x'", NO_PARAMS)
            .expect_err("cc_number_enc should be invalid");
    }
}

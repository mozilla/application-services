/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::sql_fns;
use crate::sync::address::name_utils::{join_name_parts, NameParts};
use error_support::debug;
use rusqlite::{functions::FunctionFlags, Connection, Transaction};
use sql_support::open_database::{ConnectionInitializer, Error, Result};

pub const ADDRESS_COMMON_COLS: &str = "
    guid,
    name,
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
    :name,
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

pub struct AutofillConnectionInitializer;

impl ConnectionInitializer for AutofillConnectionInitializer {
    const NAME: &'static str = "autofill db";
    const END_VERSION: u32 = 3;

    fn prepare(&self, conn: &Connection, _db_empty: bool) -> Result<()> {
        define_functions(conn)?;

        let initial_pragmas = "
            -- use in-memory storage
            PRAGMA temp_store = 2;
            -- use write-ahead logging
            PRAGMA journal_mode = WAL;
            -- autofill does not use foreign keys at present but this is probably a good pragma to set
            PRAGMA foreign_keys = ON;
        ";
        conn.execute_batch(initial_pragmas)?;

        conn.set_prepared_statement_cache_capacity(128);
        Ok(())
    }

    fn init(&self, db: &Transaction<'_>) -> Result<()> {
        Ok(db.execute_batch(CREATE_SHARED_SCHEMA_SQL)?)
    }

    fn upgrade_from(&self, db: &Transaction<'_>, version: u32) -> Result<()> {
        match version {
            // AutofillDB has a slightly strange version history, so we start on v0.  See
            // upgrade_from_v0() for more details.
            0 => upgrade_from_v0(db),
            1 => upgrade_from_v1(db),
            2 => upgrade_from_v2(db),
            _ => Err(Error::IncompatibleVersion(version)),
        }
    }

    fn finish(&self, db: &Connection) -> Result<()> {
        Ok(db.execute_batch(CREATE_SHARED_TRIGGERS_SQL)?)
    }
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

fn upgrade_from_v0(db: &Connection) -> Result<()> {
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

fn upgrade_from_v1(db: &Connection) -> Result<()> {
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

fn upgrade_from_v2(db: &Connection) -> Result<()> {
    db.execute_batch("ALTER TABLE addresses_data ADD COLUMN name TEXT NOT NULL DEFAULT ''")?;

    let mut stmt =
        db.prepare("SELECT guid, given_name, additional_name, family_name FROM addresses_data")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>("guid")?,
            row.get::<_, String>("given_name")?,
            row.get::<_, String>("additional_name")?,
            row.get::<_, String>("family_name")?,
        ))
    })?;

    for row in rows {
        let (guid, given, middle, family) = row?;
        let full_name = join_name_parts(&NameParts {
            given,
            middle,
            family,
        });

        db.execute(
            "UPDATE addresses_data SET name = (:name) WHERE guid = (:guid)",
            rusqlite::named_params! { ":name": full_name, ":guid": guid},
        )?;
    }

    db.execute_batch(
        "
        ALTER TABLE addresses_data DROP COLUMN given_name;
        ALTER TABLE addresses_data DROP COLUMN additional_name;
        ALTER TABLE addresses_data DROP COLUMN family_name;
        ",
    )?;

    Ok(())
}

pub fn create_empty_sync_temp_tables(db: &Connection) -> Result<()> {
    debug!("Initializing sync temp tables");
    db.execute_batch(CREATE_SYNC_TEMP_TABLES_SQL)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::addresses::get_address;
    use crate::db::credit_cards::get_credit_card;
    use crate::db::test::new_mem_db;
    use sql_support::open_database::test_utils::MigratedDatabaseFile;
    use sync_guid::Guid;
    use types::Timestamp;

    const CREATE_V0_DB: &str = include_str!("../../sql/tests/create_v0_db.sql");
    const CREATE_V1_DB: &str = include_str!("../../sql/tests/create_v1_db.sql");
    const CREATE_V2_DB: &str = include_str!("../../sql/tests/create_v2_db.sql");

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
        // Let's start with v1, since the v0 upgrade deletes data
        let db_file = MigratedDatabaseFile::new(AutofillConnectionInitializer, CREATE_V1_DB);
        db_file.run_all_upgrades();
        let conn = db_file.open();

        // Test that the data made it through
        let cc = get_credit_card(&conn, &Guid::new("A")).unwrap();
        assert_eq!(cc.guid, "A");
        assert_eq!(cc.cc_name, "Jane Doe");
        assert_eq!(cc.cc_number_enc, "012345678901234567890");
        assert_eq!(cc.cc_number_last_4, "1234");
        assert_eq!(cc.cc_exp_month, 1);
        assert_eq!(cc.cc_exp_year, 2020);
        assert_eq!(cc.cc_type, "visa");
        assert_eq!(cc.metadata.time_created, Timestamp(0));
        assert_eq!(cc.metadata.time_last_used, Timestamp(1));
        assert_eq!(cc.metadata.time_last_modified, Timestamp(2));
        assert_eq!(cc.metadata.times_used, 3);
        assert_eq!(cc.metadata.sync_change_counter, 0);

        let address = get_address(&conn, &Guid::new("A")).unwrap();
        assert_eq!(address.guid, "A");
        assert_eq!(address.name, "Jane JaneDoe2 Doe");
        assert_eq!(address.organization, "Mozilla");
        assert_eq!(address.street_address, "123 Maple lane");
        assert_eq!(address.address_level3, "Shelbyville");
        assert_eq!(address.address_level2, "Springfield");
        assert_eq!(address.address_level1, "MA");
        assert_eq!(address.postal_code, "12345");
        assert_eq!(address.country, "US");
        assert_eq!(address.tel, "01-234-567-8000");
        assert_eq!(address.email, "jane@hotmail.com");
        assert_eq!(address.metadata.time_created, Timestamp(0));
        assert_eq!(address.metadata.time_last_used, Timestamp(1));
        assert_eq!(address.metadata.time_last_modified, Timestamp(2));
        assert_eq!(address.metadata.times_used, 3);
        assert_eq!(address.metadata.sync_change_counter, 0);
    }

    #[test]
    fn test_upgrade_version_0() {
        let db_file = MigratedDatabaseFile::new(AutofillConnectionInitializer, CREATE_V0_DB);
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
        let db_file = MigratedDatabaseFile::new(AutofillConnectionInitializer, CREATE_V1_DB);

        db_file.upgrade_to(2);
        let db = db_file.open();

        // Test the upgraded check constraint
        db.execute("UPDATE credit_cards_data SET cc_number_enc=''", [])
            .expect("blank cc_number_enc should be valid");
        db.execute("UPDATE credit_cards_data SET cc_number_enc='x'", [])
            .expect_err("cc_number_enc should be invalid");
    }

    #[test]
    fn test_upgrade_version_2() {
        let db_file = MigratedDatabaseFile::new(AutofillConnectionInitializer, CREATE_V2_DB);
        let db = db_file.open();

        db.execute_batch("SELECT name from addresses_data")
            .expect_err("select should fail");
        db.execute_batch("SELECT street_address from addresses_data")
            .expect("street_address should work");
        db.execute_batch("SELECT additional_name from addresses_data")
            .expect("additional_name should work");
        db.execute_batch("SELECT family_name from addresses_data")
            .expect("family_name should work");

        db_file.upgrade_to(3);

        db.execute_batch("SELECT name from addresses_data")
            .expect("select name should now work");
        db.execute_batch("SELECT given_name from addresses_data")
            .expect_err("given_name should fail");
        db.execute_batch("SELECT additional_name from addresses_data")
            .expect_err("additional_name should fail");
        db.execute_batch("SELECT family_name from addresses_data")
            .expect_err("family_name should fail");

        let mut address = get_address(&db, &Guid::new("A")).unwrap();
        assert_eq!(address.guid, "A");
        assert_eq!(address.name, "Jane John Doe");

        address = get_address(&db, &Guid::new("B")).unwrap();
        assert_eq!(address.guid, "B");

        // Record B has no given_name, additional_name or family_name, so name should also be empty.
        assert_eq!(address.name, "");
    }
}

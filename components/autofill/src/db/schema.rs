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
const VERSION: i64 = 1;

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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test::new_mem_db;

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
            PRAGMA user_version = 0;
        ",
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
}

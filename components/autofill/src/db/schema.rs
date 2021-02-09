/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::Result;
use rusqlite::Connection;

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
    cc_number,
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
    :cc_number,
    :cc_exp_month,
    :cc_exp_year,
    :cc_type,
    :time_created,
    :time_last_used,
    :time_last_modified,
    :times_used";

#[allow(dead_code)]
const CREATE_SHARED_SCHEMA_SQL: &str = include_str!("../../sql/create_shared_schema.sql");
const CREATE_SHARED_TRIGGERS_SQL: &str = include_str!("../../sql/create_shared_triggers.sql");
const CREATE_SYNC_TEMP_TABLES_SQL: &str = include_str!("../../sql/create_sync_temp_tables.sql");

#[allow(dead_code)]
pub fn init(db: &Connection) -> Result<()> {
    create(db)?;
    Ok(())
}

#[allow(dead_code)]
fn create(db: &Connection) -> Result<()> {
    log::debug!("Creating schema");
    db.execute_batch(
        format!(
            "{}\n{}",
            CREATE_SHARED_SCHEMA_SQL, CREATE_SHARED_TRIGGERS_SQL
        )
        .as_str(),
    )?;

    Ok(())
}

pub fn create_empty_sync_temp_tables(db: &Connection) -> Result<()> {
    log::debug!("Initializing sync temp tables");
    db.execute_batch(CREATE_SYNC_TEMP_TABLES_SQL)?;
    Ok(())
}

/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod incoming;

use crate::error::*;
use rusqlite::Row;
use serde::Serialize;
use serde_derive::*;
use sync_guid::Guid as SyncGuid;
use types::Timestamp;

const RECORD_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(default)]
pub struct AddressRecord {
    #[serde(rename = "id")]
    pub guid: SyncGuid,
    pub given_name: String,
    pub additional_name: String,
    pub family_name: String,
    pub organization: String,
    pub street_address: String,
    pub address_level3: String,
    pub address_level2: String,
    pub address_level1: String,
    pub postal_code: String,
    pub country: String,
    pub tel: String,
    pub email: String,
    // metadata isn't kebab-case for some reason...
    #[serde(rename = "timeCreated")]
    pub time_created: Timestamp,
    #[serde(rename = "timeLastUsed")]
    pub time_last_used: Timestamp,
    #[serde(rename = "timeLastModified")]
    pub time_last_modified: Timestamp,
    #[serde(rename = "timesUsed")]
    pub times_used: i64,
    // The server stores a "version" field that's always 1
    pub version: u32,
    // Change counter is never serialized or deserialized.
    #[serde(skip)]
    pub sync_change_counter: Option<i64>,
}

impl AddressRecord {
    pub fn from_row(row: &Row<'_>, column_prefix: &str) -> Result<AddressRecord> {
        Ok(AddressRecord {
            guid: row.get::<_, SyncGuid>(format!("{}{}", column_prefix, "guid").as_str())?,
            given_name: row
                .get::<_, String>(format!("{}{}", column_prefix, "given_name").as_str())?,
            additional_name: row
                .get::<_, String>(format!("{}{}", column_prefix, "additional_name").as_str())?,
            family_name: row
                .get::<_, String>(format!("{}{}", column_prefix, "family_name").as_str())?,
            organization: row
                .get::<_, String>(format!("{}{}", column_prefix, "organization").as_str())?,
            street_address: row
                .get::<_, String>(format!("{}{}", column_prefix, "street_address").as_str())?,
            address_level3: row
                .get::<_, String>(format!("{}{}", column_prefix, "address_level3").as_str())?,
            address_level2: row
                .get::<_, String>(format!("{}{}", column_prefix, "address_level2").as_str())?,
            address_level1: row
                .get::<_, String>(format!("{}{}", column_prefix, "address_level1").as_str())?,
            postal_code: row
                .get::<_, String>(format!("{}{}", column_prefix, "postal_code").as_str())?,
            country: row.get::<_, String>(format!("{}{}", column_prefix, "country").as_str())?,
            tel: row.get::<_, String>(format!("{}{}", column_prefix, "tel").as_str())?,
            email: row.get::<_, String>(format!("{}{}", column_prefix, "email").as_str())?,
            time_created: row.get(format!("{}{}", column_prefix, "time_created").as_str())?,
            time_last_used: row.get(format!("{}{}", column_prefix, "time_last_used").as_str())?,
            time_last_modified: row
                .get(format!("{}{}", column_prefix, "time_last_modified").as_str())?,
            times_used: row.get(format!("{}{}", column_prefix, "times_used").as_str())?,
            version: RECORD_VERSION,
            sync_change_counter: row
                .get(format!("{}{}", column_prefix, "sync_change_counter").as_str())
                .ok(),
        })
    }
}

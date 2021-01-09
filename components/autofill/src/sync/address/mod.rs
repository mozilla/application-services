/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod incoming;

use rusqlite::{types::FromSql, Row};
use serde::Serialize;
use serde_derive::*;
use sync_guid::Guid as SyncGuid;
use types::Timestamp;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RecordData {
    #[serde(default)]
    pub given_name: String,

    #[serde(default)]
    pub additional_name: String,

    #[serde(default)]
    pub family_name: String,

    #[serde(default)]
    pub organization: String,

    #[serde(default)]
    pub street_address: String,

    #[serde(default)]
    pub address_level3: String,

    #[serde(default)]
    pub address_level2: String,

    #[serde(default)]
    pub address_level1: String,

    #[serde(default)]
    pub postal_code: String,

    #[serde(default)]
    pub country: String,

    #[serde(default)]
    pub tel: String,

    #[serde(default)]
    pub email: String,

    #[serde(default)]
    pub time_created: Option<Timestamp>,

    #[serde(default)]
    pub time_last_used: Option<Timestamp>,

    #[serde(default)]
    pub time_last_modified: Option<Timestamp>,

    #[serde(default)]
    pub times_used: Option<i64>,

    #[serde(default)]
    pub sync_change_counter: Option<i64>,
}

impl RecordData {
    pub fn from_row(row: &Row<'_>) -> Result<RecordData, rusqlite::Error> {
        Ok(RecordData {
            given_name: row.get::<_, String>("given_name")?,
            additional_name: row.get::<_, String>("additional_name")?,
            family_name: row.get::<_, String>("family_name")?,
            organization: row.get::<_, String>("organization")?,
            street_address: row.get::<_, String>("street_address")?,
            address_level3: row.get::<_, String>("address_level3")?,
            address_level2: row.get::<_, String>("address_level2")?,
            address_level1: row.get::<_, String>("address_level1")?,
            postal_code: row.get::<_, String>("postal_code")?,
            country: row.get::<_, String>("country")?,
            tel: row.get::<_, String>("tel")?,
            email: row.get::<_, String>("email")?,
            time_created: FromSql::column_result(row.get_raw::<_>("time_created")).ok(),
            time_last_used: FromSql::column_result(row.get_raw::<_>("time_last_used")).ok(),
            time_last_modified: FromSql::column_result(row.get_raw::<_>("time_last_modified")).ok(),
            times_used: FromSql::column_result(row.get_raw::<_>("times_used")).ok(),
            sync_change_counter: FromSql::column_result(row.get_raw::<_>("sync_change_counter"))
                .ok(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    #[serde(rename = "id", default)]
    pub guid: SyncGuid,

    #[serde(flatten)]
    data: RecordData,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddressChanges {
    // #[serde(rename = "id")]
    // pub guid: SyncGuid,
    pub old_value: Option<Record>,

    pub new_value: Option<Record>,
}

/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

pub mod engine;
pub mod incoming;

use super::{Metadata, SyncRecord};
use crate::error::*;
use rusqlite::Row;
use serde::Serialize;
use serde_derive::*;
use sync_guid::Guid as SyncGuid;

const RECORD_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(default)]
struct AddressRecord {
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
    #[serde(flatten)]
    pub metadata: Metadata,
}

impl AddressRecord {}

impl SyncRecord for AddressRecord {
    fn record_name() -> &'static str {
        "Address"
    }

    fn id(&self) -> &SyncGuid {
        &self.guid
    }

    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }

    fn from_row(row: &Row<'_>, column_prefix: &str) -> Result<AddressRecord> {
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
            metadata: Metadata {
                time_created: row.get(format!("{}{}", column_prefix, "time_created").as_str())?,
                time_last_used: row
                    .get(format!("{}{}", column_prefix, "time_last_used").as_str())?,
                time_last_modified: row
                    .get(format!("{}{}", column_prefix, "time_last_modified").as_str())?,
                times_used: row.get(format!("{}{}", column_prefix, "times_used").as_str())?,
                version: RECORD_VERSION,
                sync_change_counter: row
                    .get(format!("{}{}", column_prefix, "sync_change_counter").as_str())
                    .ok(),
            },
        })
    }
}

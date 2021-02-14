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

    fn from_row(row: &Row<'_>) -> Result<AddressRecord> {
        Ok(AddressRecord {
            guid: row.get::<_, SyncGuid>("guid")?,
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
            metadata: Metadata {
                time_created: row.get("time_created")?,
                time_last_used: row.get("time_last_used")?,
                time_last_modified: row.get("time_last_modified")?,
                times_used: row.get("times_used")?,
                version: RECORD_VERSION,
                sync_change_counter: row.get("sync_change_counter")?,
            },
        })
    }
}

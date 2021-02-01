/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use rusqlite::Row;
use serde::Serialize;
use serde_derive::*;
use sync_guid::Guid;
use types::Timestamp;

#[derive(Debug, Clone, Hash, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case", default)]
pub struct NewAddressFields {
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
}

#[derive(Debug, Clone, Hash, PartialEq, Serialize, Deserialize, Default)]
pub struct InternalAddress {
    pub guid: Guid,

    pub fields: NewAddressFields,

    #[serde(default)]
    #[serde(rename = "timeCreated")]
    pub time_created: Timestamp,

    #[serde(default)]
    #[serde(rename = "timeLastUsed")]
    pub time_last_used: Option<Timestamp>,

    #[serde(default)]
    #[serde(rename = "timeLastModified")]
    pub time_last_modified: Timestamp,

    #[serde(default)]
    #[serde(rename = "timesUsed")]
    pub times_used: i64,

    #[serde(default)]
    #[serde(rename = "changeCounter")]
    pub(crate) sync_change_counter: i64,
}

impl InternalAddress {
    pub fn from_row(row: &Row<'_>) -> Result<InternalAddress, rusqlite::Error> {
        let address_fields = NewAddressFields {
            given_name: row.get("given_name")?,
            additional_name: row.get("additional_name")?,
            family_name: row.get("family_name")?,
            organization: row.get("organization")?,
            street_address: row.get("street_address")?,
            address_level3: row.get("address_level3")?,
            address_level2: row.get("address_level2")?,
            address_level1: row.get("address_level1")?,
            postal_code: row.get("postal_code")?,
            country: row.get("country")?,
            tel: row.get("tel")?,
            email: row.get("email")?,
        };

        Ok(InternalAddress {
            guid: Guid::from_string(row.get("guid")?),
            fields: address_fields,
            time_created: row.get("time_created")?,
            time_last_used: row.get("time_last_used")?,
            time_last_modified: row.get("time_last_modified")?,
            times_used: row.get("times_used")?,
            sync_change_counter: row.get("sync_change_counter")?,
        })
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct Address {
    pub guid: String,
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
}

pub trait ExternalizeAddress {
    fn to_external(&self) -> Address;
}

impl ExternalizeAddress for InternalAddress {
    fn to_external(&self) -> Address {
        Address {
            guid: self.guid.to_string(),
            given_name: self.fields.given_name.to_string(),
            additional_name: self.fields.additional_name.to_string(),
            family_name: self.fields.family_name.to_string(),
            organization: self.fields.organization.to_string(),
            street_address: self.fields.street_address.to_string(),
            address_level3: self.fields.address_level3.to_string(),
            address_level2: self.fields.address_level2.to_string(),
            address_level1: self.fields.address_level1.to_string(),
            postal_code: self.fields.postal_code.to_string(),
            country: self.fields.country.to_string(),
            tel: self.fields.tel.to_string(),
            email: self.fields.email.to_string(),
        }
    }
}

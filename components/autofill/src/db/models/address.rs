/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use rusqlite::Row;
use serde::Serialize;
use serde_derive::*;
use sync_guid::Guid;
use types::Timestamp;

// UpdatableAddressFields contains the fields we support for creating a new
// address or updating an existing one. It's missing the guid, our "internal"
// meta fields (such as the change counter) and "external" meta fields
// (such as timeCreated) because it doesn't make sense for these things to be
// specified as an item is created - any meta fields which can be updated
// have special methods for doing so.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct UpdatableAddressFields {
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

// "Address" is what we return to consumers and has most of the metadata.
#[derive(Debug, Clone, Hash, PartialEq, Default)]
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
    // We expose the metadata
    pub time_created: i64,
    pub time_last_used: Option<i64>,
    pub time_last_modified: i64,
    pub times_used: i64,
}

// This is used to "externalize" an address, suitable for handing back to
// consumers.
impl From<InternalAddress> for Address {
    fn from(ia: InternalAddress) -> Self {
        Address {
            guid: ia.guid.to_string(),
            given_name: ia.given_name,
            additional_name: ia.additional_name,
            family_name: ia.family_name,
            organization: ia.organization,
            street_address: ia.street_address,
            address_level3: ia.address_level3,
            address_level2: ia.address_level2,
            address_level1: ia.address_level1,
            postal_code: ia.postal_code,
            country: ia.country,
            tel: ia.tel,
            email: ia.email,
            // *sob* - can't use u64 in uniffi
            time_created: u64::from(ia.time_created) as i64,
            time_last_used: ia.time_last_used.map(|v| u64::from(v) as i64),
            time_last_modified: u64::from(ia.time_last_modified) as i64,
            times_used: ia.times_used,
        }
    }
}

// An "internal" address has both the fields we expose to consumers and those
// we do not. This is the primary struct used internally, and is serialized to
// and from JSON for sync etc.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct InternalAddress {
    // it sucks that we need to duplicate the fields, but we do so because
    // uniffi forces us to use, eg, strings for guids and ints for timestamps,
    // but we want our rust code to deal with timestamps etc.
    pub guid: Guid,
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

    // We expose the metadata - note that for compatibility with desktop
    // these are *not* kebab-case - for some obscure reason the sync server's
    // records have a mix of cases.
    #[serde(rename = "timeCreated")]
    pub time_created: Timestamp,
    #[serde(rename = "timeLastUsed")]
    pub time_last_used: Option<Timestamp>,
    #[serde(rename = "timeLastModified")]
    pub time_last_modified: Timestamp,
    #[serde(rename = "timesUsed")]
    pub times_used: i64,

    #[serde(default)]
    #[serde(rename = "changeCounter")]
    pub(crate) sync_change_counter: i64,
}

impl InternalAddress {
    pub fn from_row(row: &Row<'_>) -> Result<InternalAddress, rusqlite::Error> {
        Ok(Self {
            guid: row.get("guid")?,
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
            time_created: row.get("time_created")?,
            time_last_used: row.get("time_last_used")?,
            time_last_modified: row.get("time_last_modified")?,
            times_used: row.get("times_used")?,
            sync_change_counter: row.get("sync_change_counter")?,
        })
    }
}

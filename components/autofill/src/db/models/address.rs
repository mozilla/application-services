/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::Metadata;
use rusqlite::Row;
use sync_guid::Guid;

// UpdatableAddressFields contains the fields we support for creating a new
// address or updating an existing one. It's missing the guid, our "internal"
// meta fields (such as the change counter) and "external" meta fields
// (such as timeCreated) because it doesn't make sense for these things to be
// specified as an item is created - any meta fields which can be updated
// have special methods for doing so.
#[derive(Debug, Clone, Default)]
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
#[derive(Debug, Clone, Hash, PartialEq, Eq, Default)]
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
    // We expose some of the metadata
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
            // note we can't use u64 in uniffi
            time_created: u64::from(ia.metadata.time_created) as i64,
            time_last_used: if ia.metadata.time_last_used.0 == 0 {
                None
            } else {
                Some(ia.metadata.time_last_used.0 as i64)
            },
            time_last_modified: u64::from(ia.metadata.time_last_modified) as i64,
            times_used: ia.metadata.times_used,
        }
    }
}

// An "internal" address is used by the public APIs and by sync. No `PartialEq`
// because it's impossible to do it meaningfully for credit-cards and we'd like
// to keep the API symmetric
#[derive(Default, Debug, Clone)]
pub struct InternalAddress {
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
    pub metadata: Metadata,
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
            metadata: Metadata {
                time_created: row.get("time_created")?,
                time_last_used: row.get("time_last_used")?,
                time_last_modified: row.get("time_last_modified")?,
                times_used: row.get("times_used")?,
                sync_change_counter: row.get("sync_change_counter")?,
            },
        })
    }
}

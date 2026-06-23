/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::Metadata;
use rusqlite::Row;
use sync_guid::Guid;

// What you pass to create or update a passport.
#[derive(Debug, Clone, Default)]
pub struct UpdatablePassportFields {
    pub name: String,
    pub country: String,
    pub passport_number: String,
    pub issue_date_month: i64,
    pub issue_date_day: i64,
    pub issue_date_year: i64,
    pub expiry_date_month: i64,
    pub expiry_date_day: i64,
    pub expiry_date_year: i64,
}

// "Passport" is what we return to consumers and has most of the metadata.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Default)]
pub struct Passport {
    pub guid: String,
    pub name: String,
    pub country: String,
    pub passport_number: String,
    pub issue_date_month: i64,
    pub issue_date_day: i64,
    pub issue_date_year: i64,
    pub expiry_date_month: i64,
    pub expiry_date_day: i64,
    pub expiry_date_year: i64,

    // We expose some of the metadata
    pub time_created: i64,
    pub time_last_used: Option<i64>,
    pub time_last_modified: i64,
    pub times_used: i64,
}

// This is used to "externalize" a passport, suitable for handing back to
// consumers.
impl From<InternalPassport> for Passport {
    fn from(ip: InternalPassport) -> Self {
        Passport {
            guid: ip.guid.to_string(),
            name: ip.name,
            country: ip.country,
            passport_number: ip.passport_number,
            issue_date_month: ip.issue_date_month,
            issue_date_day: ip.issue_date_day,
            issue_date_year: ip.issue_date_year,
            expiry_date_month: ip.expiry_date_month,
            expiry_date_day: ip.expiry_date_day,
            expiry_date_year: ip.expiry_date_year,
            // note we can't use u64 in uniffi
            time_created: u64::from(ip.metadata.time_created) as i64,
            time_last_used: if ip.metadata.time_last_used.0 == 0 {
                None
            } else {
                Some(ip.metadata.time_last_used.0 as i64)
            },
            time_last_modified: u64::from(ip.metadata.time_last_modified) as i64,
            times_used: ip.metadata.times_used,
        }
    }
}

// An "internal" passport is used by the public APIs and by sync.
#[derive(Debug, Clone, Default)]
pub struct InternalPassport {
    pub guid: Guid,
    pub name: String,
    pub country: String,
    pub passport_number: String,
    pub issue_date_month: i64,
    pub issue_date_day: i64,
    pub issue_date_year: i64,
    pub expiry_date_month: i64,
    pub expiry_date_day: i64,
    pub expiry_date_year: i64,
    pub metadata: Metadata,
}

impl InternalPassport {
    pub fn from_row(row: &Row<'_>) -> Result<InternalPassport, rusqlite::Error> {
        Ok(Self {
            guid: Guid::from_string(row.get("guid")?),
            name: row.get("name")?,
            country: row.get("country")?,
            passport_number: row.get("passport_number")?,
            issue_date_month: row.get("issue_date_month")?,
            issue_date_day: row.get("issue_date_day")?,
            issue_date_year: row.get("issue_date_year")?,
            expiry_date_month: row.get("expiry_date_month")?,
            expiry_date_day: row.get("expiry_date_day")?,
            expiry_date_year: row.get("expiry_date_year")?,
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

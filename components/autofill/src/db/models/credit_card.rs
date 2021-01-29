/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use rusqlite::Row;
use serde::Serialize;
use serde_derive::*;
use sync_guid::Guid;
use types::Timestamp;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct UpdatableCreditCardFields {
    pub cc_name: String,
    pub cc_number: String,
    pub cc_exp_month: i64,
    pub cc_exp_year: i64,
    // Credit card types are a fixed set of strings as defined in the link below
    // (https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
    pub cc_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct CreditCard {
    pub guid: String,
    pub cc_name: String,
    pub cc_number: String,
    pub cc_exp_month: i64,
    pub cc_exp_year: i64,

    // Credit card types are a fixed set of strings as defined in the link below
    // (https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
    pub cc_type: String,

    // The metadata
    pub time_created: i64,
    pub time_last_used: Option<i64>,
    pub time_last_modified: i64,
    pub times_used: i64,
}

// This is used to "externalize" a credit-card, suitable for handing back to
// consumers.
impl From<InternalCreditCard> for CreditCard {
    fn from(icc: InternalCreditCard) -> Self {
        CreditCard {
            guid: icc.guid.to_string(),
            cc_name: icc.cc_name,
            cc_number: icc.cc_number,
            cc_exp_month: icc.cc_exp_month,
            cc_exp_year: icc.cc_exp_year,
            cc_type: icc.cc_type,
            // *sob* - can't use u64 in uniffi
            time_created: u64::from(icc.time_created) as i64,
            time_last_used: icc.time_last_used.map(|v| u64::from(v) as i64),
            time_last_modified: u64::from(icc.time_last_modified) as i64,
            times_used: icc.times_used,
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "kebab-case")]
pub struct InternalCreditCard {
    pub guid: Guid,
    pub cc_name: String,
    pub cc_number: String,
    pub cc_exp_month: i64,
    pub cc_exp_year: i64,
    // Credit card types are a fixed set of strings as defined in the link below
    // (https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
    pub cc_type: String,

    #[serde(rename = "timeCreated")]
    pub time_created: Timestamp,
    #[serde(rename = "timeLastUsed")]
    pub time_last_used: Option<Timestamp>,
    #[serde(rename = "timeLastModified")]
    pub time_last_modified: Timestamp,
    #[serde(rename = "timesUsed")]
    pub times_used: i64,
    #[serde(rename = "changeCounter")]
    pub(crate) sync_change_counter: i64,
}

impl InternalCreditCard {
    pub fn from_row(row: &Row<'_>) -> Result<InternalCreditCard, rusqlite::Error> {
        Ok(Self {
            guid: Guid::from_string(row.get("guid")?),
            cc_name: row.get("cc_name")?,
            cc_number: row.get("cc_number")?,
            cc_exp_month: row.get("cc_exp_month")?,
            cc_exp_year: row.get("cc_exp_year")?,
            cc_type: row.get("cc_type")?,
            time_created: row.get("time_created")?,
            time_last_used: row.get("time_last_used")?,
            time_last_modified: row.get("time_last_modified")?,
            times_used: row.get("times_used")?,
            sync_change_counter: row.get("sync_change_counter")?,
        })
    }
}

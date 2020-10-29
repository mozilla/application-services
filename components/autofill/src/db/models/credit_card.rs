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
#[serde(rename_all = "kebab-case")]
pub struct NewCreditCardFields {
    pub cc_name: String,

    pub cc_number: String,

    pub cc_exp_month: i64,

    pub cc_exp_year: i64,

    // Credit card types are a fixed set of strings as defined in the link below
    // (https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
    pub cc_type: String,
}

#[derive(Debug, Clone, Hash, PartialEq, Serialize, Deserialize, Default)]
pub struct InternalCreditCard {
    pub guid: Guid,

    pub fields: NewCreditCardFields,

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

impl InternalCreditCard {
    pub fn from_row(row: &Row<'_>) -> Result<InternalCreditCard, rusqlite::Error> {
        let credit_card_fields = NewCreditCardFields {
            cc_name: row.get("cc_name")?,
            cc_number: row.get("cc_number")?,
            cc_exp_month: row.get("cc_exp_month")?,
            cc_exp_year: row.get("cc_exp_year")?,
            cc_type: row.get("cc_type")?,
        };

        Ok(InternalCreditCard {
            guid: Guid::from_string(row.get("guid")?),
            fields: credit_card_fields,
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
pub struct CreditCard {
    pub guid: String,
    pub cc_name: String,
    pub cc_number: String,
    pub cc_exp_month: i64,
    pub cc_exp_year: i64,

    // Credit card types are a fixed set of strings as defined in the link below
    // (https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
    pub cc_type: String,
}

pub trait ExternalizeCreditCard {
    fn to_external(&self) -> CreditCard;
}

impl ExternalizeCreditCard for InternalCreditCard {
    fn to_external(&self) -> CreditCard {
        CreditCard {
            guid: self.guid.to_string(),
            cc_name: self.clone().fields.cc_name,
            cc_number: self.clone().fields.cc_number,
            cc_exp_month: self.fields.cc_exp_month,
            cc_exp_year: self.fields.cc_exp_year,
            cc_type: self.clone().fields.cc_type,
        }
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::Metadata;
use rusqlite::Row;
use sync_guid::Guid;

/// This struct represents credit card data to add records to the database or update existing records.
#[derive(Debug, Clone, Default)]
pub struct UpdatableCreditCardFields {
    /// The full name of the credit card holder.
    pub cc_name: String,
    /// The encrypted credit card number stored as a JWE. This value has a length greater than 20 or will be an
    /// empty string if the credit card has been scrubbed because the encryption key was lost.
    pub cc_number_enc: String,
    /// The last four digits of the credit card number, unencrypted.
    pub cc_number_last_4: String,
    /// The last four digits of the credit card number, unencrypted.
    pub cc_exp_month: i64,
    /// The last four digits of the credit card number, unencrypted.
    pub cc_exp_year: i64,
    /// The credit card type, one of the values in a fixed set of strings as defined
    /// [for Desktop](https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
    pub cc_type: String,
}

/// This struct represents credit card data returned from the database.
#[derive(Debug, Clone, Default)]
pub struct CreditCard {
    /// The unique ID of the credit card.
    pub guid: String,
    /// The full name of the credit card holder.
    pub cc_name: String,
    /// The encrypted credit card number stored as a JWE. This value has a length greater than 20 or will be an
    /// empty string if the credit card has been scrubbed because the encryption key was lost.
    pub cc_number_enc: String,
    /// The last four digits of the credit card number, unencrypted.
    pub cc_number_last_4: String,
    /// The last four digits of the credit card number, unencrypted.
    pub cc_exp_month: i64,
    /// The last four digits of the credit card number, unencrypted.
    pub cc_exp_year: i64,
    /// The credit card type, one of the values in a fixed set of strings as defined
    /// [for Desktop](https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
    pub cc_type: String,

    // The metadata
    /// The time the credit card was created.
    pub time_created: i64,
    /// The time the credit card was last used or [`touch`]ed, [`None`] if never used.
    pub time_last_used: Option<i64>,
    /// The time the credit card was last changed.
    pub time_last_modified: i64,
    /// The number of times the credit card was used or [`touch`]ed.
    pub times_used: i64,
}

// This is used to "externalize" a credit-card, suitable for handing back to
// consumers.
impl From<InternalCreditCard> for CreditCard {
    fn from(icc: InternalCreditCard) -> Self {
        CreditCard {
            guid: icc.guid.to_string(),
            cc_name: icc.cc_name,
            cc_number_enc: icc.cc_number_enc,
            cc_number_last_4: icc.cc_number_last_4,
            cc_exp_month: icc.cc_exp_month,
            cc_exp_year: icc.cc_exp_year,
            cc_type: icc.cc_type,
            // note we can't use u64 in uniffi
            time_created: u64::from(icc.metadata.time_created) as i64,
            time_last_used: if icc.metadata.time_last_used.0 == 0 {
                None
            } else {
                Some(icc.metadata.time_last_used.0 as i64)
            },
            time_last_modified: u64::from(icc.metadata.time_last_modified) as i64,
            times_used: icc.metadata.times_used,
        }
    }
}

// NOTE: No `PartialEq` here because the same card number will encrypt to a
// different value each time it is encrypted, making it meaningless to compare.
#[derive(Debug, Clone, Default)]
pub struct InternalCreditCard {
    pub guid: Guid,
    pub cc_name: String,
    pub cc_number_enc: String,
    pub cc_number_last_4: String,
    pub cc_exp_month: i64,
    pub cc_exp_year: i64,
    // Credit card types are a fixed set of strings as defined in the link below
    // (https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
    pub cc_type: String,
    pub metadata: Metadata,
}

impl InternalCreditCard {
    pub fn from_row(row: &Row<'_>) -> Result<InternalCreditCard, rusqlite::Error> {
        Ok(Self {
            guid: Guid::from_string(row.get("guid")?),
            cc_name: row.get("cc_name")?,
            cc_number_enc: row.get("cc_number_enc")?,
            cc_number_last_4: row.get("cc_number_last_4")?,
            cc_exp_month: row.get("cc_exp_month")?,
            cc_exp_year: row.get("cc_exp_year")?,
            cc_type: row.get("cc_type")?,
            metadata: Metadata {
                time_created: row.get("time_created")?,
                time_last_used: row.get("time_last_used")?,
                time_last_modified: row.get("time_last_modified")?,
                times_used: row.get("times_used")?,
                sync_change_counter: row.get("sync_change_counter")?,
            },
        })
    }

    pub fn has_scrubbed_data(&self) -> bool {
        self.cc_number_enc.is_empty()
    }
}

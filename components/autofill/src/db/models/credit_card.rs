/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use super::Metadata;
use rusqlite::Row;
use sync_guid::Guid;
use types::Timestamp;

#[derive(Debug, Clone, Default)]
pub struct UpdatableCreditCardFields {
    pub cc_name: String,
    pub cc_number_enc: String,
    pub cc_number_last_4: String,
    pub cc_exp_month: i64,
    pub cc_exp_year: i64,
    // Credit card types are a fixed set of strings as defined in the link below
    // (https://searchfox.org/mozilla-central/rev/7ef5cefd0468b8f509efe38e0212de2398f4c8b3/toolkit/modules/CreditCard.jsm#9-22)
    pub cc_type: String,
}

/// Metadata fields managed internally by the library: the guid, timestamps and
/// local sync state. These are automatically set on `add_credit_card` and
/// updated on operations like `touch` and `update_credit_card`. Not included in
/// `UpdatableCreditCardFields`; use `add_credit_card_with_meta` when importing
/// records that already have metadata.
#[derive(Debug, Clone, Default)]
pub struct CreditCardMeta {
    pub guid: String,
    pub time_created: i64,
    pub time_last_used: Option<i64>,
    pub time_last_modified: i64,
    pub times_used: i64,
    /// Local changes not yet uploaded; 0 means it matches what was last synced.
    pub sync_change_counter: i64,
}

/// A tombstone for a record deleted locally but not yet uploaded, supplied to
/// `add_many_credit_card_tombstones` when migrating from another store.
#[derive(Debug, Clone, Default)]
pub struct CreditCardTombstone {
    pub guid: String,
    pub time_deleted: i64,
}

/// Per-record result of `add_many_credit_card_tombstones`.
#[derive(Debug)]
pub enum CreditCardBulkTombstoneResultEntry {
    Success { guid: String },
    Error { message: String },
}

/// A credit card together with its metadata, passed to
/// `add_credit_card_with_meta` and `update_credit_card_with_meta` when importing
/// a record from another store.
#[derive(Debug, Clone, Default)]
pub struct UpdatableCreditCardFieldsWithMeta {
    pub fields: UpdatableCreditCardFields,
    pub meta: CreditCardMeta,
}

/// A bulk insert result entry, returned per input record by
/// `add_many_credit_cards_with_meta` so that one record failing does not abort
/// the batch. Note that although the success case is much larger than the error
/// case, this is negligible in real life, as we expect a very small
/// success/error ratio.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum CreditCardBulkResultEntry {
    Success { credit_card: CreditCard },
    Error { message: String },
}

#[derive(Debug, Clone, Default)]
pub struct CreditCard {
    pub guid: String,
    pub cc_name: String,
    pub cc_number_enc: String,
    pub cc_number_last_4: String,
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
                time_created: row.get::<_, Timestamp>("time_created")?.sanitized(),
                time_last_used: row.get::<_, Timestamp>("time_last_used")?.sanitized(),
                time_last_modified: row.get::<_, Timestamp>("time_last_modified")?.sanitized(),
                times_used: row.get("times_used")?,
                sync_change_counter: row.get("sync_change_counter")?,
            },
        })
    }

    pub fn has_scrubbed_data(&self) -> bool {
        self.cc_number_enc.is_empty()
    }
}

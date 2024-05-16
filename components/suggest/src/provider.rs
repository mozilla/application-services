/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use rusqlite::{
    types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef},
    Result as RusqliteResult,
};

use crate::rs::SuggestRecordType;

/// A provider is a source of search suggestions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum SuggestionProvider {
    Amp = 1,
    Wikipedia = 2,
    Amo = 3,
    Pocket = 4,
    Yelp = 5,
    Mdn = 6,
    Weather = 7,
    AmpMobile = 8,
}

impl FromSql for SuggestionProvider {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        u8::try_from(v)
            .ok()
            .and_then(SuggestionProvider::from_u8)
            .ok_or_else(|| FromSqlError::OutOfRange(v))
    }
}

impl SuggestionProvider {
    pub fn all() -> [Self; 8] {
        [
            Self::Amp,
            Self::Wikipedia,
            Self::Amo,
            Self::Pocket,
            Self::Yelp,
            Self::Mdn,
            Self::Weather,
            Self::AmpMobile,
        ]
    }

    #[inline]
    pub(crate) fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(SuggestionProvider::Amp),
            2 => Some(SuggestionProvider::Wikipedia),
            3 => Some(SuggestionProvider::Amo),
            4 => Some(SuggestionProvider::Pocket),
            5 => Some(SuggestionProvider::Yelp),
            6 => Some(SuggestionProvider::Mdn),
            7 => Some(SuggestionProvider::Weather),
            _ => None,
        }
    }

    pub(crate) fn records_for_provider(&self) -> Vec<SuggestRecordType> {
        match self {
            SuggestionProvider::Amp => {
                vec![
                    SuggestRecordType::AmpWikipedia,
                    SuggestRecordType::Icon,
                    SuggestRecordType::GlobalConfig,
                ]
            }
            SuggestionProvider::Wikipedia => {
                vec![
                    SuggestRecordType::AmpWikipedia,
                    SuggestRecordType::Icon,
                    SuggestRecordType::GlobalConfig,
                ]
            }
            SuggestionProvider::Amo => {
                vec![SuggestRecordType::Amo, SuggestRecordType::GlobalConfig]
            }
            SuggestionProvider::Pocket => {
                vec![SuggestRecordType::Pocket, SuggestRecordType::GlobalConfig]
            }
            SuggestionProvider::Yelp => {
                vec![
                    SuggestRecordType::Yelp,
                    SuggestRecordType::Icon,
                    SuggestRecordType::GlobalConfig,
                ]
            }
            SuggestionProvider::Mdn => {
                vec![SuggestRecordType::Mdn, SuggestRecordType::GlobalConfig]
            }
            SuggestionProvider::Weather => {
                vec![SuggestRecordType::Weather, SuggestRecordType::GlobalConfig]
            }
            SuggestionProvider::AmpMobile => {
                vec![
                    SuggestRecordType::AmpMobile,
                    SuggestRecordType::AmpWikipedia,
                    SuggestRecordType::Icon,
                    SuggestRecordType::GlobalConfig,
                ]
            }
        }
    }
}

impl ToSql for SuggestionProvider {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(*self as u8))
    }
}

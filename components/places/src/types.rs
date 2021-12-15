/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::Result as RusqliteResult;
use serde::ser::{Serialize, Serializer};
use std::convert::TryFrom;
use std::fmt;

mod visit_transition_set;
pub use visit_transition_set::VisitTransitionSet;

#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[error("Invalid visit type")]
pub struct InvalidVisitType;

// NOTE: These discriminator values are the same as those used by Desktop
// Firefox and are what is written to the database. We also duplicate them
// as constants in visit_transition_set.rs
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum VisitType {
    UpdatePlace = -1,
    // This transition type means the user followed a link.
    Link = 1,

    // This transition type means that the user typed the page's URL in the
    // URL bar or selected it from UI (URL bar autocomplete results, etc)
    Typed = 2,

    // XXX - moar comments.
    Bookmark = 3,
    Embed = 4,
    RedirectPermanent = 5,
    RedirectTemporary = 6,
    Download = 7,
    FramedLink = 8,
    Reload = 9,
}

impl ToSql for VisitType {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(*self as u8))
    }
}

impl VisitType {
    pub fn from_primitive(p: u8) -> Option<Self> {
        match p {
            1 => Some(VisitType::Link),
            2 => Some(VisitType::Typed),
            3 => Some(VisitType::Bookmark),
            4 => Some(VisitType::Embed),
            5 => Some(VisitType::RedirectPermanent),
            6 => Some(VisitType::RedirectTemporary),
            7 => Some(VisitType::Download),
            8 => Some(VisitType::FramedLink),
            9 => Some(VisitType::Reload),
            _ => None,
        }
    }
}

impl TryFrom<u8> for VisitType {
    type Error = InvalidVisitType;
    fn try_from(p: u8) -> Result<Self, Self::Error> {
        VisitType::from_primitive(p).ok_or(InvalidVisitType)
    }
}

struct VisitTransitionSerdeVisitor;

impl<'de> serde::de::Visitor<'de> for VisitTransitionSerdeVisitor {
    type Value = VisitType;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("positive integer representing VisitTransition")
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<VisitType, E> {
        use std::u8::MAX as U8_MAX;
        if value > u64::from(U8_MAX) {
            // In practice this is *way* out of the valid range of VisitTransition, but
            // serde requires us to implement this as visit_u64 so...
            return Err(E::custom(format!("value out of u8 range: {}", value)));
        }
        VisitType::from_primitive(value as u8)
            .ok_or_else(|| E::custom(format!("unknown VisitTransition value: {}", value)))
    }
}

impl serde::Serialize for VisitType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(*self as u64)
    }
}

impl<'de> serde::Deserialize<'de> for VisitType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_u64(VisitTransitionSerdeVisitor)
    }
}

/// Bookmark types.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub enum BookmarkType {
    Bookmark = 1, // TYPE_BOOKMARK
    Folder = 2,   // TYPE_FOLDER
    Separator = 3, // TYPE_SEPARATOR;
                  // On desktop, TYPE_DYNAMIC_CONTAINER = 4 but is deprecated - so please
                  // avoid using this value in the future.
}

impl FromSql for BookmarkType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        if v < 0 || v > i64::from(u8::max_value()) {
            return Err(FromSqlError::OutOfRange(v));
        }
        BookmarkType::from_u8(v as u8).ok_or(FromSqlError::OutOfRange(v))
    }
}

impl BookmarkType {
    #[inline]
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(BookmarkType::Bookmark),
            2 => Some(BookmarkType::Folder),
            3 => Some(BookmarkType::Separator),
            _ => None,
        }
    }

    pub fn from_u8_with_valid_url<F: Fn() -> bool>(v: u8, has_valid_url: F) -> Self {
        match BookmarkType::from_u8(v) {
            Some(BookmarkType::Bookmark) | None => {
                if has_valid_url() {
                    // Even if the node says it is a bookmark it still must have a
                    // valid url.
                    BookmarkType::Bookmark
                } else {
                    BookmarkType::Folder
                }
            }
            Some(t) => t,
        }
    }
}

impl ToSql for BookmarkType {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(*self as u8))
    }
}

impl Serialize for BookmarkType {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

/// Re SyncStatus - note that:
/// * logins has synced=0, changed=1, new=2
/// * desktop bookmarks has unknown=0, new=1, normal=2
/// This is "places", so eventually bookmarks will have a status - should history
/// and bookmarks share this enum?
/// Note that history specifically needs neither (a) login's "changed" (the
/// changeCounter works there), nor (b) bookmark's "unknown" (as that's only
/// used after a restore).
/// History only needs a distinction between "synced" and "new" so it doesn't
/// accumulate never-to-be-synced tombstones - so we basically copy bookmarks
/// and treat unknown as new.
/// Which means we get the "bonus side-effect" ;) of ::Unknown replacing Option<>!
///
/// Note that some of these values are in schema.rs
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub enum SyncStatus {
    Unknown = 0,
    New = 1,
    Normal = 2,
}

impl SyncStatus {
    #[inline]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => SyncStatus::New,
            2 => SyncStatus::Normal,
            _ => SyncStatus::Unknown,
        }
    }
}

impl FromSql for SyncStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let v = value.as_i64()?;
        if v < 0 || v > i64::from(u8::max_value()) {
            return Err(FromSqlError::OutOfRange(v));
        }
        Ok(SyncStatus::from_u8(v as u8))
    }
}

impl ToSql for SyncStatus {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(*self as u8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive() {
        assert_eq!(Some(VisitType::Link), VisitType::from_primitive(1));
        assert_eq!(None, VisitType::from_primitive(99));
    }
}

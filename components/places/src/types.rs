/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{fmt};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{types::{ToSql, FromSql, ToSqlOutput, FromSqlResult, ValueRef}};
use rusqlite::Result as RusqliteResult;

use serde;

// XXX - copied from logins - surprised it's not in `sync`
#[derive(PartialEq, Eq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct SyncGuid(pub String);

impl AsRef<str> for SyncGuid {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl<T> From<T> for SyncGuid where T: Into<String> {
    fn from(x: T) -> SyncGuid {
        SyncGuid(x.into())
    }
}

impl ToSql for SyncGuid {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput> {
        Ok(ToSqlOutput::from(self.0.clone())) // cloning seems wrong?
    }
}

impl FromSql for SyncGuid {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        value.as_str().map(|v| SyncGuid(v.to_string()))
    }
}

// Typesafe way to manage timestamps.
// We should probably work out how to share this too?
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize, Default)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub fn now() -> Self {
        SystemTime::now().into()
    }
}

impl From<Timestamp> for u64 {
    #[inline]
    fn from(ts: Timestamp) -> Self { ts.0 }
}

impl From<SystemTime> for Timestamp {
    #[inline]
    fn from(st: SystemTime) -> Self {
        let d = st.duration_since(UNIX_EPOCH).unwrap(); // hrmph - unwrap doesn't seem ideal
        Timestamp((d.as_secs() as u64) * 1000 + ((d.subsec_nanos() as u64) / 1_000_000))
    }
}

impl From<u64> for Timestamp {
    #[inline]
    fn from(ts: u64) -> Self {
        assert!(ts != 0);
        Timestamp(ts)
    }
}

impl fmt::Display for Timestamp {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ToSql for Timestamp {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput> {
        Ok(ToSqlOutput::from(self.0 as i64)) // hrm - no u64 in rusqlite
    }
}

impl FromSql for Timestamp {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        value.as_i64().map(|v| Timestamp(v as u64)) // hrm - no u64
    }
}


// NOTE: These discriminator values are the same as those used by Desktop
// Firefox and are what is written to the database. We also duplicate them
// in the android lib as constants on PlacesConnection.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum VisitTransition {
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

impl ToSql for VisitTransition {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput> {
        Ok(ToSqlOutput::from(*self as u8))
    }
}

impl VisitTransition {
    pub fn from_primitive(p: u8) -> Option<Self> {
        match p {
            1 => Some(VisitTransition::Link),
            2 => Some(VisitTransition::Typed),
            3 => Some(VisitTransition::Bookmark),
            4 => Some(VisitTransition::Embed),
            5 => Some(VisitTransition::RedirectPermanent),
            6 => Some(VisitTransition::RedirectTemporary),
            7 => Some(VisitTransition::Download),
            8 => Some(VisitTransition::FramedLink),
            9 => Some(VisitTransition::Reload),
            _ => None,
        }
    }
}

struct VisitTransitionSerdeVisitor;

impl<'de> serde::de::Visitor<'de> for VisitTransitionSerdeVisitor {
    type Value = VisitTransition;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("positive integer representing VisitTransition")
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<VisitTransition, E> {
        use std::u8::{MAX as U8_MAX};
        if value > (U8_MAX as u64) {
            // In practice this is *way* out of the valid range of VisitTransition, but
            // serde requires us to implement this as visit_u64 so...
            return Err(E::custom(format!("value out of u8 range: {}", value)));
        }
        VisitTransition::from_primitive(value as u8).ok_or_else(||
            E::custom(format!("unknown VisitTransition value: {}", value)))
    }
}

impl serde::Serialize for VisitTransition {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(*self as u64)
    }
}

impl<'de> serde::Deserialize<'de> for VisitTransition {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_u64(VisitTransitionSerdeVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive() {
        assert_eq!(Some(VisitTransition::Link), VisitTransition::from_primitive(1));
        assert_eq!(None, VisitTransition::from_primitive(99));
    }
}

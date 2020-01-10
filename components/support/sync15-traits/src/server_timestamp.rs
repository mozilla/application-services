/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use std::marker::PhantomData;
use std::time::Duration;

/// Typesafe way to manage server timestamps without accidentally mixing them up with
/// local ones.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Default)]
pub struct ServerTimestamp(pub i64);

impl From<ServerTimestamp> for i64 {
    #[inline]
    fn from(ts: ServerTimestamp) -> Self {
        ts.0
    }
}

impl From<ServerTimestamp> for f64 {
    #[inline]
    fn from(ts: ServerTimestamp) -> Self {
        ts.0 as f64 / 1000.0
    }
}

impl From<i64> for ServerTimestamp {
    #[inline]
    fn from(ts: i64) -> Self {
        ServerTimestamp(ts)
    }
}

impl From<f64> for ServerTimestamp {
    fn from(ts: f64) -> Self {
        let rf = (ts * 1000.0).round();
        if !rf.is_finite() || rf < 0.0 || rf >= i64::max_value() as f64 {
            log::error!("Illegal timestamp: {}", ts);
            ServerTimestamp(0)
        } else {
            ServerTimestamp(rf as i64)
        }
    }
}

// This lets us use these in hyper header! blocks.
impl std::str::FromStr for ServerTimestamp {
    type Err = std::num::ParseFloatError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let val = f64::from_str(s)?;
        Ok(val.into())
    }
}

impl std::fmt::Display for ServerTimestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0 as f64 / 1000.0)
    }
}

impl ServerTimestamp {
    pub const EPOCH: ServerTimestamp = ServerTimestamp(0);

    /// Returns None if `other` is later than `self` (Duration may not represent
    /// negative timespans in rust).
    #[inline]
    pub fn duration_since(self, other: ServerTimestamp) -> Option<Duration> {
        let delta = self.0 - other.0;
        if delta < 0 {
            None
        } else {
            Some(Duration::from_millis(delta as u64))
        }
    }

    /// Get the milliseconds for the timestamp.
    #[inline]
    pub fn as_millis(self) -> i64 {
        self.0
    }
}

impl serde::ser::Serialize for ServerTimestamp {
    fn serialize<S: serde::ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.0 as f64 / 1000.0)
    }
}

struct TimestampVisitor(PhantomData<ServerTimestamp>);

impl<'de> serde::de::Visitor<'de> for TimestampVisitor {
    type Value = ServerTimestamp;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("A 64 bit float number value.")
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
        Ok(value.into())
    }
}

impl<'de> serde::de::Deserialize<'de> for ServerTimestamp {
    fn deserialize<D: serde::de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_f64(TimestampVisitor(PhantomData))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_server_timestamp() {
        let t0 = ServerTimestamp(10_300_150);
        let t1 = ServerTimestamp(10_100_050);
        assert!(t1.duration_since(t0).is_none());
        assert!(t0.duration_since(t1).is_some());
        let dur = t0.duration_since(t1).unwrap();
        assert_eq!(dur.as_secs(), 200);
        assert_eq!(dur.subsec_nanos(), 100_000_000);
    }

    #[test]
    fn test_serde() {
        let ts = ServerTimestamp(123_456);

        // test serialize
        let ser = serde_json::to_string(&ts).unwrap();
        assert_eq!("123.456".to_string(), ser);

        // test deserialize
        let ts: ServerTimestamp = serde_json::from_str(&ser).unwrap();
        assert_eq!(ServerTimestamp(123_456), ts);
    }
}

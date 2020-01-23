/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde::de::{self, Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, Serializer};
use std::collections::{HashMap, HashSet};
use std::convert::From;
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use std::{fmt, num};
/// Typesafe way to manage server timestamps without accidentally mixing them up with
/// local ones.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Default)]
pub struct ServerTimestamp(pub i64);

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
impl FromStr for ServerTimestamp {
    type Err = num::ParseFloatError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let val = f64::from_str(s)?;
        Ok(val.into())
    }
}

impl fmt::Display for ServerTimestamp {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0 as f64 / 1000.0)
    }
}

/// Finds the maximum of the current value and the argument `val`, and sets the
/// new value to the result.
///
/// Note: `AtomicFoo::fetch_max` is unstable, and can't really be implemented as
/// a single atomic operation from outside the stdlib ;-;
pub(crate) fn atomic_update_max(v: &AtomicU32, new: u32) {
    // For loads (and the compare_exchange_weak second ordering argument) this
    // is too strong, we could probably get away with Acquire (or maybe Relaxed
    // because we don't need the result?). In either case, this fn isn't called
    // from a hot spot so whatever.
    let mut cur = v.load(Ordering::SeqCst);
    while cur < new {
        // we're already handling the failure case so there's no reason not to
        // use _weak here.
        match v.compare_exchange_weak(cur, new, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => {
                // Success.
                break;
            }
            Err(new_cur) => {
                // Interrupted, keep trying.
                cur = new_cur
            }
        }
    }
}

pub const SERVER_EPOCH: ServerTimestamp = ServerTimestamp(0);

impl ServerTimestamp {
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

impl Serialize for ServerTimestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(self.0 as f64 / 1000.0)
    }
}

impl<'de> Deserialize<'de> for ServerTimestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TimestampVisitor;

        impl<'de> Visitor<'de> for TimestampVisitor {
            type Value = ServerTimestamp;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("A 64 bit float number value.")
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(value.into())
            }
        }

        deserializer.deserialize_f64(TimestampVisitor)
    }
}

// Slight wrappers around the builtin methods for doing this.
pub(crate) fn set_union(a: &HashSet<String>, b: &HashSet<String>) -> HashSet<String> {
    a.union(b).cloned().collect()
}

pub(crate) fn set_difference(a: &HashSet<String>, b: &HashSet<String>) -> HashSet<String> {
    a.difference(b).cloned().collect()
}

pub(crate) fn set_intersection(a: &HashSet<String>, b: &HashSet<String>) -> HashSet<String> {
    a.intersection(b).cloned().collect()
}

pub(crate) fn partition_by_value(v: &HashMap<String, bool>) -> (HashSet<String>, HashSet<String>) {
    let mut true_: HashSet<String> = HashSet::new();
    let mut false_: HashSet<String> = HashSet::new();
    for (s, val) in v {
        if *val {
            true_.insert(s.clone());
        } else {
            false_.insert(s.clone());
        }
    }
    (true_, false_)
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
    #[test]
    fn test_set_ops() {
        fn hash_set(s: &[&str]) -> HashSet<String> {
            s.iter()
                .copied()
                .map(ToOwned::to_owned)
                .collect::<HashSet<_>>()
        }

        assert_eq!(
            set_union(&hash_set(&["a", "b", "c"]), &hash_set(&["b", "d"])),
            hash_set(&["a", "b", "c", "d"]),
        );

        assert_eq!(
            set_difference(&hash_set(&["a", "b", "c"]), &hash_set(&["b", "d"])),
            hash_set(&["a", "c"]),
        );
        assert_eq!(
            set_intersection(&hash_set(&["a", "b", "c"]), &hash_set(&["b", "d"])),
            hash_set(&["b"]),
        );
        let m: HashMap<String, bool> = [
            ("foo", true),
            ("bar", true),
            ("baz", false),
            ("quux", false),
        ]
        .iter()
        .copied()
        .map(|(a, b)| (a.to_owned(), b))
        .collect();
        assert_eq!(
            partition_by_value(&m),
            (hash_set(&["foo", "bar"]), hash_set(&["baz", "quux"])),
        );
    }
}

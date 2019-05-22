/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde_derive::*;
use std::convert::From;
use std::str::FromStr;
use std::time::Duration;
use std::{fmt, num};

pub fn random_guid() -> Result<String, openssl::error::ErrorStack> {
    let mut bytes = vec![0u8; 9];
    openssl::rand::rand_bytes(&mut bytes)?;
    Ok(base64::encode_config(&bytes, base64::URL_SAFE_NO_PAD))
}

/// Typesafe way to manage server timestamps without accidentally mixing them up with
/// local ones.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Deserialize, Serialize, Default)]
pub struct ServerTimestamp(pub i64);

impl From<ServerTimestamp> for i64 {
    #[inline]
    fn from(ts: ServerTimestamp) -> Self {
        ts.0
    }
}

impl From<i64> for ServerTimestamp {
    #[inline]
    fn from(ts: i64) -> Self {
        ServerTimestamp(ts)
    }
}

// This lets us use these in hyper header! blocks.
impl FromStr for ServerTimestamp {
    type Err = num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ServerTimestamp(i64::from_str(s)?))
    }
}

impl fmt::Display for ServerTimestamp {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
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

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashSet;

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
    fn test_gen_guid() {
        let mut set = HashSet::new();
        for _ in 0..100 {
            let res = random_guid().unwrap();
            assert_eq!(res.len(), 12);
            assert!(!set.contains(&res));
            set.insert(res);
        }
    }
}

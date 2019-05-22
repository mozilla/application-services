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
///
/// TODO: We should probably store this as milliseconds (or something) for stability and to get
/// Eq/Ord. The server guarantees that these are formatted to the hundreds place (not sure if this
/// is documented but the code does it intentionally...). This would also let us throw out negative
/// and NaN timestamps, which the server certainly won't send, but the guarantee would make me feel
/// better.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Deserialize, Serialize, Default)]
pub struct ServerTimestamp(pub f64);

impl From<ServerTimestamp> for f64 {
    #[inline]
    fn from(ts: ServerTimestamp) -> Self {
        ts.0
    }
}

impl From<f64> for ServerTimestamp {
    #[inline]
    fn from(ts: f64) -> Self {
        assert!(ts >= 0.0);
        ServerTimestamp(ts)
    }
}

// This lets us use these in hyper header! blocks.
impl FromStr for ServerTimestamp {
    type Err = num::ParseFloatError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ServerTimestamp(f64::from_str(s)?))
    }
}

impl fmt::Display for ServerTimestamp {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub const SERVER_EPOCH: ServerTimestamp = ServerTimestamp(0.0);

impl ServerTimestamp {
    /// Returns None if `other` is later than `self` (Duration may not represent
    /// negative timespans in rust).
    #[inline]
    pub fn duration_since(self, other: ServerTimestamp) -> Option<Duration> {
        let delta = self.0 - other.0;
        if delta < 0.0 {
            None
        } else {
            let secs = delta.floor();
            // We don't want to round here, since it could round up, and
            // Duration::new will panic if it rounds up to 1e9 nanoseconds.
            let nanos = ((delta - secs) * 1_000_000_000.0).floor() as u32;
            Some(Duration::new(secs as u64, nanos))
        }
    }

    /// Get the milliseconds for the timestamp.
    #[inline]
    pub fn as_millis(self) -> u64 {
        (self.0 * 1000.0).floor() as u64
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_server_timestamp() {
        let t0 = ServerTimestamp(10300.15);
        let t1 = ServerTimestamp(10100.05);
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

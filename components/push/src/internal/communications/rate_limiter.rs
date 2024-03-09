/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::{info, warn};
use crate::internal::storage::Storage;
use std::{
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

// DB persisted rate limiter.
// Implementation notes: This saves the timestamp of our latest call and the number of times we have
// called `Self::check` within the `Self::periodic_interval` interval of time.
pub struct PersistedRateLimiter {
    op_name: String,
    periodic_interval: u64, // In seconds.
    max_requests_in_interval: u16,
}

impl PersistedRateLimiter {
    pub fn new(op_name: &str, periodic_interval: u64, max_requests_in_interval: u16) -> Self {
        Self {
            op_name: op_name.to_owned(),
            periodic_interval,
            max_requests_in_interval,
        }
    }

    pub fn check<S: Storage>(&self, store: &S) -> bool {
        let (mut timestamp, mut count) = self.impl_get_counters(store);

        let now = now_secs();
        if (now - timestamp) >= self.periodic_interval {
            info!(
                "Resetting. now({}) - {} < {} for {}.",
                now, timestamp, self.periodic_interval, &self.op_name
            );
            count = 0;
            timestamp = now;
        } else {
            info!(
                "No need to reset inner timestamp and count for {}.",
                &self.op_name
            )
        }

        count += 1;
        self.impl_persist_counters(store, timestamp, count);

        // within interval counter
        if count > self.max_requests_in_interval {
            info!(
                "Not allowed: count({}) > {} for {}.",
                count, self.max_requests_in_interval, &self.op_name
            );
            return false;
        }

        info!("Allowed to pass through for {}!", &self.op_name);

        true
    }

    pub fn reset<S: Storage>(&self, store: &S) {
        self.impl_persist_counters(store, now_secs(), 0)
    }

    fn db_meta_keys(&self) -> (String, String) {
        (
            format!("ratelimit_{}_timestamp", &self.op_name),
            format!("ratelimit_{}_count", &self.op_name),
        )
    }

    fn impl_get_counters<S: Storage>(&self, store: &S) -> (u64, u16) {
        let (timestamp_key, count_key) = self.db_meta_keys();
        (
            Self::get_meta_integer(store, &timestamp_key),
            Self::get_meta_integer(store, &count_key),
        )
    }

    #[cfg(test)]
    pub(crate) fn get_counters<S: Storage>(&self, store: &S) -> (u64, u16) {
        self.impl_get_counters(store)
    }

    fn get_meta_integer<S: Storage, T: FromStr + Default>(store: &S, key: &str) -> T {
        store
            .get_meta(key)
            .ok()
            .flatten()
            .map(|s| s.parse())
            .transpose()
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    fn impl_persist_counters<S: Storage>(&self, store: &S, timestamp: u64, count: u16) {
        let (timestamp_key, count_key) = self.db_meta_keys();
        let r1 = store.set_meta(&timestamp_key, &timestamp.to_string());
        let r2 = store.set_meta(&count_key, &count.to_string());
        if r1.is_err() || r2.is_err() {
            warn!("Error updating persisted counters for {}.", &self.op_name);
        }
    }

    #[cfg(test)]
    pub(crate) fn persist_counters<S: Storage>(&self, store: &S, timestamp: u64, count: u16) {
        self.impl_persist_counters(store, timestamp, count)
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Current date before unix epoch.")
        .as_secs()
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::error::Result;
    use crate::Store;

    static PERIODIC_INTERVAL: u64 = 24 * 3600;
    static VERIFY_NOW_INTERVAL: u64 = PERIODIC_INTERVAL + 3600;
    static MAX_REQUESTS: u16 = 500;

    #[test]
    fn test_persisted_rate_limiter_store_counters_roundtrip() -> Result<()> {
        let limiter = PersistedRateLimiter::new("op1", PERIODIC_INTERVAL, MAX_REQUESTS);
        let store = Store::open_in_memory()?;
        limiter.impl_persist_counters(&store, 123, 321);
        assert_eq!((123, 321), limiter.impl_get_counters(&store));
        Ok(())
    }

    #[test]
    fn test_persisted_rate_limiter_after_interval_counter_resets() -> Result<()> {
        let limiter = PersistedRateLimiter::new("op1", PERIODIC_INTERVAL, MAX_REQUESTS);
        let store = Store::open_in_memory()?;
        limiter.impl_persist_counters(&store, now_secs() - VERIFY_NOW_INTERVAL, 50);
        assert!(limiter.check(&store));
        assert_eq!(1, limiter.impl_get_counters(&store).1);
        Ok(())
    }

    #[test]
    fn test_persisted_rate_limiter_false_above_rate_limit() -> Result<()> {
        let limiter = PersistedRateLimiter::new("op1", PERIODIC_INTERVAL, MAX_REQUESTS);
        let store = Store::open_in_memory()?;
        limiter.impl_persist_counters(&store, now_secs(), MAX_REQUESTS + 1);
        assert!(!limiter.check(&store));
        assert_eq!(MAX_REQUESTS + 2, limiter.impl_get_counters(&store).1);
        Ok(())
    }

    #[test]
    fn test_persisted_rate_limiter_reset_above_rate_limit_and_interval() -> Result<()> {
        let limiter = PersistedRateLimiter::new("op1", PERIODIC_INTERVAL, MAX_REQUESTS);
        let store = Store::open_in_memory()?;
        limiter.impl_persist_counters(&store, now_secs() - VERIFY_NOW_INTERVAL, 501);
        assert!(limiter.check(&store));
        assert_eq!(1, limiter.impl_get_counters(&store).1);
        Ok(())
    }

    #[test]
    fn test_persisted_rate_limiter_no_reset_with_rate_limits() -> Result<()> {
        let limiter = PersistedRateLimiter::new("op1", PERIODIC_INTERVAL, MAX_REQUESTS);
        let store = Store::open_in_memory()?;
        assert!(limiter.check(&store));
        Ok(())
    }
}

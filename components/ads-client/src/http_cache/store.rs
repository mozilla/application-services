/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{sync::Arc, time::Duration};

use crate::{
    clock::{CacheClock, Clock},
    http_cache::{request_hash::RequestHash, ByteSize},
};
use sql_support::open_database;
use viaduct::Response;

// TODO: Should the faults be in the trait? seemsl ike test only
// TODO: Reorganize this
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultKind {
    Cleanup,
    Lookup,
    None,
    Store,
    Trim,
}

// Generic HttpCacheDb trait reflecting all the functions that are required.
// This allows implementation by both:
// - sqlite database
// - in-memory structural 'database' (as a fallback)
pub trait HttpCacheDb: Send + Sync + 'static {
    fn close(&self);
    fn clear_all(&self) -> Result<usize, open_database::Error>;
    /// Returns total size of the cache in bytes.
    fn current_total_size_bytes(&self) -> Result<ByteSize, open_database::Error>;
    /// Removes all entries from the store whose expires_at is at or before the current time.
    fn delete_expired_entries(&self, clock: Arc<dyn Clock>) -> Result<usize, open_database::Error>;

    fn invalidate_by_hash(&self, request_hash: &RequestHash)
        -> Result<usize, open_database::Error>;

    /// Lookup is agnostic to expiration. If it exists in the store, it will return the result.
    fn lookup(&self, request_hash: &RequestHash) -> Result<Option<Response>, open_database::Error>;

    /// Upsert an object into the store with an expires_at defined by the given ttl_seconds.
    /// Calling this method will always store an object regardless of headers or policy.
    /// Logic to determine the correct ttl or cache/no-cache should happen before calling this.
    fn store_with_ttl(
        &self,
        request_hash: &RequestHash,
        response: &Response,
        ttl: &Duration,
        clock: Arc<dyn Clock>,
    ) -> Result<(), open_database::Error>;

    fn delete_oldest_cached_row(&self) -> Result<(), open_database::Error>;

    /// Returns whether the db is in memory rather than backed by a file.
    fn is_memory(&self) -> Result<bool, open_database::Error>;
    /// Returns whether the db is backed by sqlite, rather than an in-Rust store.
    fn is_sqlite(&self) -> bool;

    #[cfg(test)]
    fn fetch_timestamps(&self, hash: &RequestHash)
        -> Result<(i64, i64, i64), open_database::Error>;
}

pub struct HttpCacheStore {
    clock: Arc<dyn Clock>,
    db: Box<dyn HttpCacheDb>,
    #[cfg(test)]
    fault: parking_lot::Mutex<FaultKind>,
}

impl HttpCacheStore {
    pub fn new(db: Box<dyn HttpCacheDb>) -> Self {
        Self {
            clock: Arc::new(CacheClock),
            db,
            #[cfg(test)]
            fault: parking_lot::Mutex::new(FaultKind::None),
        }
    }

    #[cfg(test)]
    pub fn new_with_test_clock(db: Box<dyn HttpCacheDb>) -> Self {
        use crate::clock::TestClock;

        Self {
            clock: Arc::new(TestClock::new(chrono::Utc::now().timestamp())),
            db,
            #[cfg(test)]
            fault: parking_lot::Mutex::new(FaultKind::None),
        }
    }

    /// Removes all entries from cache.
    pub fn clear_all(&self) -> Result<usize, open_database::Error> {
        self.db.clear_all()
    }

    pub fn close(&self) {
        self.db.close();
    }

    /// Returns whether the db is in memory rather than backed by a file.
    pub fn is_memory(&self) -> Result<bool, open_database::Error> {
        self.db.is_memory()
    }
    /// Returns whether the db is backed by sqlite, rather than an in-Rust store.
    pub fn is_sqlite(&self) -> bool {
        self.db.is_sqlite()
    }
    /// Returns total size of the cache in bytes.
    pub fn current_total_size_bytes(&self) -> Result<ByteSize, open_database::Error> {
        self.db.current_total_size_bytes()
    }

    /// Removes all entries from the store whose expires_at is at or before the current time.
    pub fn delete_expired_entries(&self) -> Result<usize, open_database::Error> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Cleanup {
            return Err(Self::forced_fault_error("forced cleanup failure"))?;
        }
        self.db.delete_expired_entries(self.clock.clone())
    }

    #[cfg(test)]
    pub fn get_clock(&self) -> &dyn Clock {
        &*self.clock
    }

    pub fn invalidate_by_hash(
        &self,
        request_hash: &RequestHash,
    ) -> Result<usize, open_database::Error> {
        self.db.invalidate_by_hash(request_hash)
    }

    /// Lookup is agnostic to expiration. If it exists in the store, it will return the result.
    pub fn lookup(
        &self,
        request_hash: &RequestHash,
    ) -> Result<Option<Response>, open_database::Error> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Lookup {
            return Err(Self::forced_fault_error("forced lookup failure"))?;
        }
        self.db.lookup(request_hash)
    }

    #[cfg(test)]
    pub fn set_fault(&self, kind: FaultKind) {
        *self.fault.lock() = kind;
    }

    /// Upsert an object into the store with an expires_at defined by the given ttl_seconds.
    /// Calling this method will always store an object regardless of headers or policy.
    /// Logic to determine the correct ttl or cache/no-cache should happen before calling this.
    pub fn store_with_ttl(
        &self,
        request_hash: &RequestHash,
        response: &Response,
        ttl: &Duration,
    ) -> Result<(), open_database::Error> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Store {
            return Err(Self::forced_fault_error("forced store failure"))?;
        }
        self.db
            .store_with_ttl(request_hash, response, ttl, self.clock.clone())?;
        Ok(())
    }

    /// Trim cache to
    pub fn trim_to_max_size(&self, max_size: &ByteSize) -> Result<(), open_database::Error> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Trim {
            return Err(Self::forced_fault_error("forced trim failure"))?;
        }
        // Repeatedly delete the oldest row until we are under `max_size`
        loop {
            let total = self.current_total_size_bytes()?;
            if total.as_u64() <= max_size.as_u64() {
                break;
            }
            println!("Trimming...");
            self.db.delete_oldest_cached_row()?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn forced_fault_error(msg: &str) -> rusqlite::Error {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::InternalMalfunction,
                extended_code: 0,
            },
            Some(msg.to_string()),
        )
    }

    #[cfg(test)]
    pub fn fetch_timestamps(
        &self,
        hash: &RequestHash,
    ) -> Result<(i64, i64, i64), open_database::Error> {
        self.db.fetch_timestamps(hash)
    }

    #[cfg(test)]
    pub fn clock(&self) -> Arc<dyn Clock> {
        self.clock.clone()
    }
}

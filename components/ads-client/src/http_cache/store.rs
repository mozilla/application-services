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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_cache::{
        connection_initializer::HttpCacheConnectionInitializer, memory::HttpCacheMemory,
    };
    use rusqlite::OpenFlags;
    use sql_support::{open_database, LazyDb};
    use std::{path::Path, time::Duration};
    use viaduct::{header_names, Headers, Method, Request, Response};

    fn hash_for_request(req: &Request) -> RequestHash {
        RequestHash::new(&(req.method.as_str(), req.url.as_str()))
    }

    fn fetch_timestamps(store: &HttpCacheStore, hash: &RequestHash) -> (i64, i64, i64) {
        store.fetch_timestamps(hash).expect("row should exist")
    }

    fn create_test_request(url: &str, body: &[u8]) -> Request {
        Request {
            body: Some(body.to_vec()),
            headers: Headers::new(),
            method: Method::Get,
            url: url.parse().unwrap(),
        }
    }

    fn create_test_response(status: u16, body: &[u8]) -> Response {
        let mut headers = Headers::new();
        headers
            .insert(header_names::CONTENT_TYPE, "application/json")
            .unwrap();

        Response {
            body: body.to_vec(),
            headers,
            request_method: Method::Get,
            status,
            url: "https://example.com/test".parse().unwrap(),
        }
    }

    fn create_test_stores() -> Vec<(&'static str, HttpCacheStore)> {
        let mut stores = vec![];

        let initializer = HttpCacheConnectionInitializer {};
        let db = LazyDb::new(
            Path::new(crate::IN_MEMORY_DB_PATH),
            OpenFlags::default(),
            initializer,
        );
        stores.push((
            "sqlite memory",
            HttpCacheStore::new_with_test_clock(Box::new(db)),
        ));

        let db = HttpCacheMemory::new();
        stores.push((
            "rust memory",
            HttpCacheStore::new_with_test_clock(Box::new(db)),
        ));

        stores
    }

    #[test]
    fn test_lookup_fault_injection() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            store.set_fault(FaultKind::Lookup);

            let req = create_test_request("https://example.com/api", b"body");
            let hash = hash_for_request(&req);
            let err = store.lookup(&hash).unwrap_err();

            match err {
                open_database::Error::SqlError(rusqlite::Error::SqliteFailure(_, Some(msg))) => {
                    assert!(msg.contains("forced lookup failure"));
                }
                other => panic!("unexpected error: {other:?}"),
            }
        }
    }

    #[test]
    fn test_store_fault_injection() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            store.set_fault(FaultKind::Store);

            let resp = create_test_response(200, b"resp");
            let hash = hash_for_request(&create_test_request("https://example.com/api", b"body"));

            let err = store
                .store_with_ttl(&hash, &resp, &Duration::from_secs(300))
                .unwrap_err();
            match err {
                open_database::Error::SqlError(rusqlite::Error::SqliteFailure(_, Some(msg))) => {
                    assert!(msg.contains("forced store failure"));
                }
                other => panic!("unexpected error: {other:?}"),
            }
        }
    }

    #[test]
    fn test_trim_fault_injection() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            store.set_fault(FaultKind::Trim);

            let req = create_test_request("https://example.com/api", b"");
            let hash = hash_for_request(&req);
            let resp = create_test_response(200, b"resp");
            store
                .store_with_ttl(&hash, &resp, &Duration::from_secs(300))
                .unwrap();

            let err = store.trim_to_max_size(&ByteSize::b(1)).unwrap_err();
            match err {
                open_database::Error::SqlError(rusqlite::Error::SqliteFailure(_, Some(msg))) => {
                    assert!(msg.contains("forced trim failure"));
                }
                other => panic!("unexpected error: {other:?}"),
            }
        }
    }
    #[test]
    fn test_cleanup_fault_injection() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            store.set_fault(FaultKind::Cleanup);

            let err = store.delete_expired_entries().unwrap_err();
            match err {
                open_database::Error::SqlError(rusqlite::Error::SqliteFailure(_, Some(msg))) => {
                    assert!(msg.contains("forced cleanup failure"));
                }
                other => panic!("unexpected error: {other:?}"),
            }
        }
    }

    #[test]
    fn test_store_with_ttl_sets_fields_consistently() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            let req = create_test_request("https://example.com/a", b"");
            let hash = hash_for_request(&req);
            let resp = create_test_response(200, b"X");

            let ttl = Duration::from_secs(5);
            store.store_with_ttl(&hash, &resp, &ttl).unwrap();

            let (cached_at, expires_at, ttl_seconds) = fetch_timestamps(&store, &hash);
            assert_eq!(ttl_seconds, ttl.as_secs() as i64);
            let diff = expires_at - cached_at;
            let ttl_seconds = ttl.as_secs();
            assert!(
                (diff == ttl_seconds as i64)
                    || (diff == ttl_seconds as i64 - 1)
                    || (diff == ttl_seconds as i64 + 1),
                "unexpected expires_at diff: got {diff}, want ~{ttl_seconds}"
            );
        }
    }

    #[test]
    fn test_upsert_refreshes_ttl_and_expiry() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            let req = create_test_request("https://example.com/b", b"");
            let hash = hash_for_request(&req);
            let resp = create_test_response(200, b"Y");

            store
                .store_with_ttl(&hash, &resp, &Duration::from_secs(300))
                .unwrap();
            let (c1, e1, t1) = fetch_timestamps(&store, &hash);
            assert_eq!(t1, 300);

            store.get_clock().advance(3);

            store
                .store_with_ttl(&hash, &resp, &Duration::from_secs(1))
                .unwrap();
            let (c2, e2, t2) = fetch_timestamps(&store, &hash);
            assert_eq!(t2, 1);
            assert!(c2 > c1);
            assert!(e2 < e1, "expires_at should move earlier when TTL shrinks");
        }
    }
    #[test]
    fn test_delete_expired_removes_only_expired() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            let req_exp = create_test_request("https://example.com/expired", b"");
            let hash_exp = hash_for_request(&req_exp);
            let req_fresh = create_test_request("https://example.com/fresh", b"");
            let hash_fresh = hash_for_request(&req_fresh);
            let resp = create_test_response(200, b"Z");

            store
                .store_with_ttl(&hash_exp, &resp, &Duration::from_secs(1))
                .unwrap();
            store
                .store_with_ttl(&hash_fresh, &resp, &Duration::from_secs(10))
                .unwrap();

            assert!(store.lookup(&hash_exp).unwrap().is_some());
            assert!(store.lookup(&hash_fresh).unwrap().is_some());

            store.clock.advance(2);
            let removed = store.delete_expired_entries().unwrap();
            assert!(
                removed >= 1,
                "expected at least one expired row to be deleted"
            );

            assert!(store.lookup(&hash_exp).unwrap().is_none());
            assert!(store.lookup(&hash_fresh).unwrap().is_some());
        }
    }

    #[test]
    fn test_lookup_is_expired_agnostic() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            let req = create_test_request("https://example.com/stale", b"");
            let hash = hash_for_request(&req);
            let resp = create_test_response(200, b"W");

            store
                .store_with_ttl(&hash, &resp, &Duration::from_secs(1))
                .unwrap();
            store.clock.advance(2);
            assert!(store.lookup(&hash).unwrap().is_some());

            store.delete_expired_entries().unwrap();
            assert!(store.lookup(&hash).unwrap().is_none());
        }
    }

    #[test]
    fn test_zero_ttl_expires_immediately_after_tick() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            let req = create_test_request("https://example.com/zero", b"");
            let hash = hash_for_request(&req);
            let resp = create_test_response(200, b"0");

            store
                .store_with_ttl(&hash, &resp, &Duration::from_secs(0))
                .unwrap();
            assert!(store.lookup(&hash).unwrap().is_some());

            store.clock.advance(2);
            let removed = store.delete_expired_entries().unwrap();
            assert!(removed >= 1);
            assert!(store.lookup(&hash).unwrap().is_none());
        }
    }

    #[test]
    fn test_store_and_retrieve() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            let request = create_test_request("https://example.com/api", b"test body");
            let hash = hash_for_request(&request);
            let response = create_test_response(200, b"test response");

            store
                .store_with_ttl(&hash, &response, &Duration::from_secs(300))
                .unwrap();

            let retrieved = store.lookup(&hash).unwrap().unwrap();
            assert_eq!(retrieved.status, 200);
            assert_eq!(retrieved.body, b"test response");
        }
    }

    #[test]
    fn test_ttl_expiration() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            let request = create_test_request("https://example.com/api", b"test body");
            let hash = hash_for_request(&request);
            let response = create_test_response(200, b"test response");

            store
                .store_with_ttl(&hash, &response, &Duration::from_secs(300))
                .unwrap();

            let retrieved = store.lookup(&hash).unwrap().unwrap();
            assert_eq!(retrieved.body, b"test response");

            store.clock.advance(2);

            let retrieved_after_expiry = store.lookup(&hash).unwrap();
            assert!(retrieved_after_expiry.is_some());
        }
    }

    #[test]
    fn test_max_size_eviction() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            for i in 0..5 {
                let request = create_test_request(&format!("https://example.com/api/{}", i), b"");
                let hash = hash_for_request(&request);
                let large_body = vec![0u8; 300];
                let response = create_test_response(200, &large_body);
                store
                    .store_with_ttl(&hash, &response, &Duration::from_secs(300))
                    .unwrap();
                store.clock().advance(1);
            }

            store.trim_to_max_size(&ByteSize::kib(1)).unwrap();

            let total_size = store.current_total_size_bytes().unwrap();
            assert!(total_size.as_u64() <= 1024);

            let first_request = create_test_request("https://example.com/api/0", b"");
            let first_hash = hash_for_request(&first_request);
            let first_cached = store.lookup(&first_hash).unwrap();
            assert!(first_cached.is_none());
        }
    }

    #[test]
    fn test_no_store_directive() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            let request = create_test_request("https://example.com/api", b"test body");
            let hash = hash_for_request(&request);

            let mut response = create_test_response(200, b"test response");
            response
                .headers
                .insert("cache-control", "no-store")
                .unwrap();

            store
                .store_with_ttl(&hash, &response, &Duration::from_secs(300))
                .unwrap();
            let retrieved = store.lookup(&hash).unwrap();
            assert!(retrieved.is_some());
        }
    }

    #[test]
    fn test_clear_all() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            let request1 = create_test_request("https://example.com/api1", b"test body 1");
            let hash1 = hash_for_request(&request1);
            let response1 = create_test_response(200, b"test response 1");
            store
                .store_with_ttl(&hash1, &response1, &Duration::from_secs(300))
                .unwrap();

            let request2 = create_test_request("https://example.com/api2", b"test body 2");
            let hash2 = hash_for_request(&request2);
            let response2 = create_test_response(200, b"test response 2");
            store
                .store_with_ttl(&hash2, &response2, &Duration::from_secs(300))
                .unwrap();

            assert!(store.lookup(&hash1).unwrap().is_some());
            assert!(store.lookup(&hash2).unwrap().is_some());

            let deleted_count = store.clear_all().unwrap();
            assert_eq!(deleted_count, 2);

            assert!(store.lookup(&hash1).unwrap().is_none());
            assert!(store.lookup(&hash2).unwrap().is_none());
        }
    }

    #[test]
    fn test_invalidate_by_hash() {
        let stores = create_test_stores();
        for (label, store) in stores {
            println!("Store: {label}");
            let request1 = create_test_request("https://example.com/api1", b"body1");
            let hash1 = hash_for_request(&request1);
            let request2 = create_test_request("https://example.com/api2", b"body2");
            let hash2 = hash_for_request(&request2);
            let resp = create_test_response(200, b"resp");

            store
                .store_with_ttl(&hash1, &resp, &Duration::from_secs(300))
                .unwrap();
            store
                .store_with_ttl(&hash2, &resp, &Duration::from_secs(300))
                .unwrap();

            assert!(store.lookup(&hash1).unwrap().is_some());
            assert!(store.lookup(&hash2).unwrap().is_some());

            let deleted = store.invalidate_by_hash(&hash1).unwrap();
            assert_eq!(deleted, 1);

            assert!(store.lookup(&hash1).unwrap().is_none());
            assert!(store.lookup(&hash2).unwrap().is_some());
        }
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::http_cache::{
    clock::{CacheClock, Clock},
    request_hash::RequestHash,
    ByteSize,
};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};
use viaduct::{Header, Response};

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultKind {
    None,
    Lookup,
    Store,
    Trim,
    Cleanup,
}

pub struct HttpCacheStore {
    conn: Mutex<Connection>,
    clock: Arc<dyn Clock>,
    #[cfg(test)]
    fault: parking_lot::Mutex<FaultKind>,
}

impl HttpCacheStore {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
            clock: Arc::new(CacheClock),
            #[cfg(test)]
            fault: parking_lot::Mutex::new(FaultKind::None),
        }
    }

    #[cfg(test)]
    pub fn new_with_test_clock(conn: Connection) -> Self {
        use crate::http_cache::clock::TestClock;

        Self {
            conn: Mutex::new(conn),
            clock: Arc::new(TestClock::new(chrono::Utc::now().timestamp())),
            #[cfg(test)]
            fault: parking_lot::Mutex::new(FaultKind::None),
        }
    }

    #[cfg(test)]
    pub fn get_clock(&self) -> &dyn Clock {
        &*self.clock
    }

    /// Removes all entries from cache.
    pub fn clear_all(&self) -> SqliteResult<usize> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM http_cache", [])
    }

    /// Returns total size of the cache in bytes.
    pub fn current_total_size_bytes(&self) -> SqliteResult<ByteSize> {
        let conn = self.conn.lock();
        let size_bytes = conn.query_row(
            "SELECT COALESCE(SUM(size_bytes),0) FROM http_cache",
            [],
            |row| row.get(0),
        )?;
        Ok(ByteSize::b(size_bytes))
    }

    /// Removes all entries from the store who's expires_at is before the current time.
    pub fn delete_expired_entries(&self) -> SqliteResult<usize> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Cleanup {
            return Err(Self::forced_fault_error("forced cleanup failure"));
        }
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM http_cache WHERE expires_at < ?1",
            params![self.clock.now_epoch_seconds()],
        )
    }

    /// Lookup is agnostic to expiration. If it exists in the store, it will return the result.
    pub fn lookup(&self, request_hash: &RequestHash) -> SqliteResult<Option<Response>> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Lookup {
            return Err(Self::forced_fault_error("forced lookup failure"));
        }
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT response_body, response_headers, response_status, request_method, request_url
             FROM http_cache WHERE request_hash = ?1",
            params![request_hash.to_string()],
            |row| {
                let response_body: Vec<u8> = row.get(0)?;
                let response_headers: Vec<u8> = row.get(1)?;
                let response_status: i64 = row.get(2)?;
                let method_str: String = row.get(3)?;
                let url_str: String = row.get(4)?;

                let headers = serde_json::from_slice::<HashMap<String, String>>(&response_headers)
                    .map(|map| {
                        map.into_iter()
                            .filter_map(|(n, v)| Header::new(n, v).ok())
                            .collect::<Vec<_>>()
                            .into()
                    })
                    .unwrap_or_else(|_| viaduct::Headers::new());

                let request_method = match method_str.as_str() {
                    "GET" => viaduct::Method::Get,
                    "HEAD" => viaduct::Method::Head,
                    "POST" => viaduct::Method::Post,
                    "PUT" => viaduct::Method::Put,
                    "DELETE" => viaduct::Method::Delete,
                    "PATCH" => viaduct::Method::Patch,
                    _ => viaduct::Method::Get,
                };

                let url = url::Url::parse(&url_str).map_err(|_| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        format!("invalid URL in cache: {url_str}").into(),
                    )
                })?;

                Ok(Response {
                    body: response_body,
                    headers,
                    request_method,
                    status: response_status as u16,
                    url,
                })
            },
        )
        .optional()
    }

    /// Upsert an object into the store with an expires_at defined by the given ttl_seconds.
    /// Calling this method will always store an object regardless of headers or policy.
    /// Logic to determine the correct ttl or cache/no-cache should happen before calling this.
    pub fn store_with_ttl(
        &self,
        request_hash: &RequestHash,
        response: &Response,
        ttl: &Duration,
    ) -> SqliteResult<()> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Store {
            return Err(Self::forced_fault_error("forced store failure"));
        }
        let headers_map: HashMap<String, String> = response.headers.clone().into();
        let response_headers = serde_json::to_vec(&headers_map).unwrap_or_default();
        let size_bytes = (response_headers.len() + response.body.len()) as i64;
        let now = self.clock.now_epoch_seconds();
        let ttl_seconds = ttl.as_secs();
        let expires_at = now + ttl_seconds as i64;

        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO http_cache (
                cached_at,
                expires_at,
                request_hash,
                request_method,
                request_url,
                response_body,
                response_headers,
                response_status,
                size_bytes,
                ttl_seconds
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(request_hash) DO UPDATE SET
                cached_at=excluded.cached_at,
                expires_at=excluded.expires_at,
                request_method=excluded.request_method,
                request_url=excluded.request_url,
                response_body=excluded.response_body,
                response_headers=excluded.response_headers,
                response_status=excluded.response_status,
                size_bytes=excluded.size_bytes,
                ttl_seconds=excluded.ttl_seconds",
            params![
                now,
                expires_at,
                request_hash.to_string(),
                response.request_method.as_str(),
                response.url.as_str(),
                response.body,
                response_headers,
                response.status,
                size_bytes,
                ttl_seconds as i64,
            ],
        )?;
        Ok(())
    }

    pub fn invalidate_by_hash(&self, request_hash: &RequestHash) -> SqliteResult<usize> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM http_cache WHERE request_hash = ?1",
            params![request_hash.to_string()],
        )
    }

    /// Trim cache to
    pub fn trim_to_max_size(&self, max_size_bytes: i64) -> SqliteResult<()> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Trim {
            return Err(Self::forced_fault_error("forced trim failure"));
        }
        loop {
            let total = self.current_total_size_bytes()?;
            if total.as_u64() <= max_size_bytes as u64 {
                break;
            }
            let conn = self.conn.lock();
            conn.execute(
                "DELETE FROM http_cache WHERE rowid IN (
                    SELECT rowid FROM http_cache ORDER BY cached_at ASC LIMIT 1
                )",
                [],
            )?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn set_fault(&self, kind: FaultKind) {
        *self.fault.lock() = kind;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_cache::connection_initializer::HttpCacheConnectionInitializer;
    use sql_support::open_database;
    use std::time::Duration;
    use viaduct::{header_names, Headers, Method, Request, Response};

    fn hash_for_request(req: &Request) -> RequestHash {
        RequestHash::new(&(req.method.as_str(), req.url.as_str()))
    }

    fn fetch_timestamps(store: &HttpCacheStore, hash: &RequestHash) -> (i64, i64, i64) {
        let conn = store.conn.lock();
        conn.query_row(
            "SELECT
                    cached_at,
                    expires_at,
                    COALESCE(ttl_seconds, -1)
            FROM http_cache WHERE request_hash = ?1",
            rusqlite::params![hash.to_string()],
            |row| {
                let cached_at: i64 = row.get(0)?;
                let expires_at: i64 = row.get(1)?;
                let ttl: i64 = row.get(2)?;
                Ok((cached_at, expires_at, ttl))
            },
        )
        .expect("row should exist")
    }

    fn create_test_request(url: &str, body: &[u8]) -> Request {
        Request {
            method: Method::Get,
            url: url.parse().unwrap(),
            headers: Headers::new(),
            body: Some(body.to_vec()),
        }
    }

    fn create_test_response(status: u16, body: &[u8]) -> Response {
        let mut headers = Headers::new();
        headers
            .insert(header_names::CONTENT_TYPE, "application/json")
            .unwrap();

        Response {
            request_method: Method::Get,
            url: "https://example.com/test".parse().unwrap(),
            status,
            headers,
            body: body.to_vec(),
        }
    }

    fn create_test_store() -> HttpCacheStore {
        let initializer = HttpCacheConnectionInitializer {};
        let conn = open_database::open_memory_database(&initializer)
            .expect("failed to open memory cache db");
        HttpCacheStore::new_with_test_clock(conn)
    }

    #[test]
    fn test_lookup_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::Lookup);

        let req = create_test_request("https://example.com/api", b"body");
        let hash = hash_for_request(&req);
        let err = store.lookup(&hash).unwrap_err();

        match err {
            rusqlite::Error::SqliteFailure(_, Some(msg)) => {
                assert!(msg.contains("forced lookup failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_store_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::Store);

        let resp = create_test_response(200, b"resp");
        let hash = hash_for_request(&create_test_request("https://example.com/api", b"body"));

        let err = store
            .store_with_ttl(&hash, &resp, &Duration::new(300, 0))
            .unwrap_err();
        match err {
            rusqlite::Error::SqliteFailure(_, Some(msg)) => {
                assert!(msg.contains("forced store failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_trim_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::Trim);

        let req = create_test_request("https://example.com/api", b"");
        let hash = hash_for_request(&req);
        let resp = create_test_response(200, b"resp");
        store
            .store_with_ttl(&hash, &resp, &Duration::new(300, 0))
            .unwrap();

        let err = store.trim_to_max_size(1).unwrap_err();
        match err {
            rusqlite::Error::SqliteFailure(_, Some(msg)) => {
                assert!(msg.contains("forced trim failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_cleanup_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::Cleanup);

        let err = store.delete_expired_entries().unwrap_err();
        match err {
            rusqlite::Error::SqliteFailure(_, Some(msg)) => {
                assert!(msg.contains("forced cleanup failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_store_with_ttl_sets_fields_consistently() {
        let store = create_test_store();
        let req = create_test_request("https://example.com/a", b"");
        let hash = hash_for_request(&req);
        let resp = create_test_response(200, b"X");

        let ttl = Duration::new(5, 0);
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

    #[test]
    fn test_upsert_refreshes_ttl_and_expiry() {
        let store = create_test_store();
        let req = create_test_request("https://example.com/b", b"");
        let hash = hash_for_request(&req);
        let resp = create_test_response(200, b"Y");

        store
            .store_with_ttl(&hash, &resp, &Duration::new(300, 0))
            .unwrap();
        let (c1, e1, t1) = fetch_timestamps(&store, &hash);
        assert_eq!(t1, 300);

        store.get_clock().advance(3);

        store
            .store_with_ttl(&hash, &resp, &Duration::new(1, 0))
            .unwrap();
        let (c2, e2, t2) = fetch_timestamps(&store, &hash);
        assert_eq!(t2, 1);
        assert!(c2 > c1);
        assert!(e2 < e1, "expires_at should move earlier when TTL shrinks");
    }

    #[test]
    fn test_delete_expired_removes_only_expired() {
        let store = create_test_store();
        let req_exp = create_test_request("https://example.com/expired", b"");
        let hash_exp = hash_for_request(&req_exp);
        let req_fresh = create_test_request("https://example.com/fresh", b"");
        let hash_fresh = hash_for_request(&req_fresh);
        let resp = create_test_response(200, b"Z");

        store
            .store_with_ttl(&hash_exp, &resp, &Duration::new(1, 0))
            .unwrap();
        store
            .store_with_ttl(&hash_fresh, &resp, &Duration::new(10, 0))
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

    #[test]
    fn test_lookup_is_expired_agnostic() {
        let store = create_test_store();
        let req = create_test_request("https://example.com/stale", b"");
        let hash = hash_for_request(&req);
        let resp = create_test_response(200, b"W");

        store
            .store_with_ttl(&hash, &resp, &Duration::new(1, 0))
            .unwrap();
        store.clock.advance(2);
        assert!(store.lookup(&hash).unwrap().is_some());

        store.delete_expired_entries().unwrap();
        assert!(store.lookup(&hash).unwrap().is_none());
    }

    #[test]
    fn test_zero_ttl_expires_immediately_after_tick() {
        let store = create_test_store();
        let req = create_test_request("https://example.com/zero", b"");
        let hash = hash_for_request(&req);
        let resp = create_test_response(200, b"0");

        store
            .store_with_ttl(&hash, &resp, &Duration::new(0, 0))
            .unwrap();
        assert!(store.lookup(&hash).unwrap().is_some());

        store.clock.advance(2);
        let removed = store.delete_expired_entries().unwrap();
        assert!(removed >= 1);
        assert!(store.lookup(&hash).unwrap().is_none());
    }

    #[test]
    fn test_store_and_retrieve() {
        let store = create_test_store();

        let request = create_test_request("https://example.com/api", b"test body");
        let hash = hash_for_request(&request);
        let response = create_test_response(200, b"test response");

        store
            .store_with_ttl(&hash, &response, &Duration::new(300, 0))
            .unwrap();

        let retrieved = store.lookup(&hash).unwrap().unwrap();
        assert_eq!(retrieved.status, 200);
        assert_eq!(retrieved.body, b"test response");
    }

    #[test]
    fn test_ttl_expiration() {
        let store = create_test_store();

        let request = create_test_request("https://example.com/api", b"test body");
        let hash = hash_for_request(&request);
        let response = create_test_response(200, b"test response");

        store
            .store_with_ttl(&hash, &response, &Duration::new(300, 0))
            .unwrap();

        let retrieved = store.lookup(&hash).unwrap().unwrap();
        assert_eq!(retrieved.body, b"test response");

        store.clock.advance(2);

        let retrieved_after_expiry = store.lookup(&hash).unwrap();
        assert!(retrieved_after_expiry.is_some());
    }

    #[test]
    fn test_max_size_eviction() {
        let initializer = HttpCacheConnectionInitializer {};
        let conn = open_database::open_memory_database(&initializer)
            .expect("failed to open memory cache db");
        let store = HttpCacheStore::new(conn);

        for i in 0..5 {
            let request = create_test_request(&format!("https://example.com/api/{}", i), b"");
            let hash = hash_for_request(&request);
            let large_body = vec![0u8; 300];
            let response = create_test_response(200, &large_body);
            store
                .store_with_ttl(&hash, &response, &Duration::new(300, 0))
                .unwrap();
        }

        store.trim_to_max_size(1024).unwrap();

        let total_size = store.current_total_size_bytes().unwrap();
        assert!(total_size.as_u64() <= 1024);

        let first_request = create_test_request("https://example.com/api/0", b"");
        let first_hash = hash_for_request(&first_request);
        let first_cached = store.lookup(&first_hash).unwrap();
        assert!(first_cached.is_none());
    }

    #[test]
    fn test_no_store_directive() {
        let store = create_test_store();
        let request = create_test_request("https://example.com/api", b"test body");
        let hash = hash_for_request(&request);

        let mut response = create_test_response(200, b"test response");
        response
            .headers
            .insert("cache-control", "no-store")
            .unwrap();

        store
            .store_with_ttl(&hash, &response, &Duration::new(300, 0))
            .unwrap();
        let retrieved = store.lookup(&hash).unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_clear_all() {
        let store = create_test_store();

        let request1 = create_test_request("https://example.com/api1", b"test body 1");
        let hash1 = hash_for_request(&request1);
        let response1 = create_test_response(200, b"test response 1");
        store
            .store_with_ttl(&hash1, &response1, &Duration::new(300, 0))
            .unwrap();

        let request2 = create_test_request("https://example.com/api2", b"test body 2");
        let hash2 = hash_for_request(&request2);
        let response2 = create_test_response(200, b"test response 2");
        store
            .store_with_ttl(&hash2, &response2, &Duration::new(300, 0))
            .unwrap();

        assert!(store.lookup(&hash1).unwrap().is_some());
        assert!(store.lookup(&hash2).unwrap().is_some());

        let deleted_count = store.clear_all().unwrap();
        assert_eq!(deleted_count, 2);

        assert!(store.lookup(&hash1).unwrap().is_none());
        assert!(store.lookup(&hash2).unwrap().is_none());
    }

    #[test]
    fn test_invalidate_by_hash() {
        let store = create_test_store();

        let request1 = create_test_request("https://example.com/api1", b"body1");
        let hash1 = hash_for_request(&request1);
        let request2 = create_test_request("https://example.com/api2", b"body2");
        let hash2 = hash_for_request(&request2);
        let resp = create_test_response(200, b"resp");

        store
            .store_with_ttl(&hash1, &resp, &Duration::new(300, 0))
            .unwrap();
        store
            .store_with_ttl(&hash2, &resp, &Duration::new(300, 0))
            .unwrap();

        assert!(store.lookup(&hash1).unwrap().is_some());
        assert!(store.lookup(&hash2).unwrap().is_some());

        let deleted = store.invalidate_by_hash(&hash1).unwrap();
        assert_eq!(deleted, 1);

        assert!(store.lookup(&hash1).unwrap().is_none());
        assert!(store.lookup(&hash2).unwrap().is_some());
    }
}

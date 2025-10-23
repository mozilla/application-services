/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;

use crate::http_cache::request_hash::RequestHash;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};
use viaduct::{Header, Request, Response};

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultKind {
    None,
    Lookup,
    Store,
    Trim,
    Cleanup,
}

pub struct HttpCacheStore {
    conn: Mutex<Connection>,
    #[cfg(test)]
    fault: parking_lot::Mutex<FaultKind>,
}

impl HttpCacheStore {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
            #[cfg(test)]
            fault: parking_lot::Mutex::new(FaultKind::None),
        }
    }

    pub fn clear_all(&self) -> SqliteResult<usize> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM http_cache", [])
    }

    pub fn current_total_size(&self) -> SqliteResult<i64> {
        let conn = self.conn.lock();
        conn.query_row("SELECT COALESCE(SUM(size),0) FROM http_cache", [], |row| {
            row.get(0)
        })
    }

    pub fn delete_expired_entries(&self, cutoff_timestamp: i64) -> SqliteResult<usize> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Cleanup {
            return Err(Self::forced_fault_error("forced cleanup failure"));
        }
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM http_cache WHERE cached_at < ?1",
            params![cutoff_timestamp],
        )
    }

    pub fn lookup(&self, request: &Request) -> SqliteResult<Option<(Response, RequestHash)>> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Lookup {
            return Err(Self::forced_fault_error("forced lookup failure"));
        }
        let request_hash = RequestHash::from(request);
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT response_body, response_headers, response_status FROM http_cache WHERE request_hash = ?1",
            params![request_hash.to_string()],
            |row| {
            let response_body = row.get(0)?;
            let response_headers: Vec<u8> = row.get(1)?;
            let response_status: i64 = row.get(2)?;
                let headers = serde_json::from_slice::<HashMap<String, String>>(&response_headers)
            .map(|map| {
                map.into_iter()
                    .filter_map(|(n, v)| Header::new(n, v).ok())
                    .collect::<Vec<_>>()
                    .into()
            })
            .unwrap_or_else(|_| viaduct::Headers::new());

        let response = Response {
            body: response_body,
            headers,
            request_method: request.method,
            status: response_status as u16,
            url: request.url.clone(),
        };
        Ok((response, request_hash))
            },
        )
        .optional()
    }

    pub fn store(&self, request: &Request, response: &Response) -> SqliteResult<RequestHash> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Store {
            return Err(Self::forced_fault_error("forced store failure"));
        }
        let request_hash = RequestHash::from(request);
        let headers_map: HashMap<String, String> = response.headers.clone().into();
        let response_headers = serde_json::to_vec(&headers_map).unwrap_or_default();
        let size = (response_headers.len() + response.body.len()) as i64;

        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO http_cache (cached_at, request_hash, response_body, response_headers, response_status, size)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(request_hash) DO UPDATE SET
                cached_at=excluded.cached_at,
                response_body=excluded.response_body,
                response_headers=excluded.response_headers,
                response_status=excluded.response_status,
                size=excluded.size",
            params![
                chrono::Utc::now().timestamp(),
                request_hash.to_string(),
                response.body,
                response_headers,
                response.status,
                size
            ],
        )?;
        Ok(request_hash)
    }

    pub fn trim_to_max_size(&self, max_size: i64) -> SqliteResult<()> {
        #[cfg(test)]
        if *self.fault.lock() == FaultKind::Trim {
            return Err(Self::forced_fault_error("forced trim failure"));
        }
        loop {
            let total = self.current_total_size()?;
            if total <= max_size {
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
        // Make a deterministic rusqlite error
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
    use viaduct::{header_names, Headers, Method, Response};

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
        HttpCacheStore::new(conn)
    }

    #[test]
    fn test_lookup_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::Lookup);

        let req = create_test_request("https://example.com/api", b"body");
        let err = store.lookup(&req).unwrap_err();

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

        let req = create_test_request("https://example.com/api", b"body");
        let resp = create_test_response(200, b"resp");

        let err = store.store(&req, &resp).unwrap_err();
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
        let resp = create_test_response(200, b"resp");
        store.store(&req, &resp).unwrap();

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

        let err = store.delete_expired_entries(0).unwrap_err();
        match err {
            rusqlite::Error::SqliteFailure(_, Some(msg)) => {
                assert!(msg.contains("forced cleanup failure"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn test_store_and_retrieve() {
        let store = create_test_store();

        let request = create_test_request("https://example.com/api", b"test body");
        let response = create_test_response(200, b"test response");

        store.store(&request, &response).unwrap();

        let retrieved = store.lookup(&request).unwrap().unwrap();
        assert_eq!(retrieved.0.status, 200);
        assert_eq!(retrieved.0.body, b"test response");
    }

    #[test]
    fn test_ttl_expiration() {
        let initializer = HttpCacheConnectionInitializer {};
        let conn = open_database::open_memory_database(&initializer)
            .expect("failed to open memory cache db");
        let store = HttpCacheStore::new(conn);

        let request = create_test_request("https://example.com/api", b"test body");
        let response = create_test_response(200, b"test response");

        store.store(&request, &response).unwrap();

        let retrieved = store.lookup(&request).unwrap().unwrap();
        assert_eq!(retrieved.0.body, b"test response");

        std::thread::sleep(Duration::from_secs(2));

        let retrieved_after_expiry = store.lookup(&request).unwrap();
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
            let large_body = vec![0u8; 300];
            let response = create_test_response(200, &large_body);
            store.store(&request, &response).unwrap();
        }

        // Trim to max size of 1024 bytes
        store.trim_to_max_size(1024).unwrap();

        let total_size = store.current_total_size().unwrap();
        assert!(total_size <= 1024);

        let first_request = create_test_request("https://example.com/api/0", b"");
        let first_cached = store.lookup(&first_request).unwrap();
        assert!(first_cached.is_none());
    }

    #[test]
    fn test_no_store_directive() {
        let store = create_test_store();
        let request = create_test_request("https://example.com/api", b"test body");

        let mut response = create_test_response(200, b"test response");
        response
            .headers
            .insert("cache-control", "no-store")
            .unwrap();

        store.store(&request, &response).unwrap();
        let retrieved = store.lookup(&request).unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_max_age_directive() {
        let store = create_test_store();
        let request = create_test_request("https://example.com/api", b"test body");

        let mut response = create_test_response(200, b"test response");
        response
            .headers
            .insert("cache-control", "max-age=1")
            .unwrap();

        store.store(&request, &response).unwrap();

        // Should be cached initially
        let retrieved = store.lookup(&request).unwrap();
        assert!(retrieved.is_some());

        // Wait for expiration
        std::thread::sleep(Duration::from_secs(2));

        // Should still be in store (expiration logic is in cache layer)
        let retrieved = store.lookup(&request).unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_clear_all() {
        let store = create_test_store();

        let request1 = create_test_request("https://example.com/api1", b"test body 1");
        let response1 = create_test_response(200, b"test response 1");
        store.store(&request1, &response1).unwrap();

        let request2 = create_test_request("https://example.com/api2", b"test body 2");
        let response2 = create_test_response(200, b"test response 2");
        store.store(&request2, &response2).unwrap();

        // Verify both are cached
        assert!(store.lookup(&request1).unwrap().is_some());
        assert!(store.lookup(&request2).unwrap().is_some());

        // Clear all entries
        let deleted_count = store.clear_all().unwrap();
        assert_eq!(deleted_count, 2);

        // Verify both are cleared
        assert!(store.lookup(&request1).unwrap().is_none());
        assert!(store.lookup(&request2).unwrap().is_none());
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::http_cache::config::HttpCacheConfigInner;
use crate::http_cache::row::HttpCacheRow;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};
use viaduct::Request;

pub struct HttpCacheStore {
    conn: Mutex<Connection>,
}

impl HttpCacheStore {
    pub fn new(_config: HttpCacheConfigInner, conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
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
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM http_cache WHERE cached_at < ?1",
            params![cutoff_timestamp],
        )
    }

    pub fn delete_oldest_entry(&self) -> SqliteResult<usize> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM http_cache WHERE rowid IN (
                SELECT rowid FROM http_cache ORDER BY cached_at ASC LIMIT 1
            )",
            [],
        )
    }

    pub fn lookup(&self, request: &Request) -> SqliteResult<Option<HttpCacheRow>> {
        let request_hash = HttpCacheRow::hash_request(request);
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT cache_control, cached_at, etag, request_hash, response_body, response_headers, response_status, size FROM http_cache WHERE request_hash = ?1",
            params![request_hash],
            HttpCacheRow::from_row,
        )
        .optional()
    }

    pub fn store(&self, cached: &HttpCacheRow) -> SqliteResult<()> {
        let cache_control_str = serde_json::to_string(&cached.cache_control).ok();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO http_cache (cache_control, cached_at, etag, request_hash, response_body, response_headers, response_status, size)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(request_hash) DO UPDATE SET
                cache_control=excluded.cache_control,
                cached_at=excluded.cached_at,
                etag=excluded.etag,
                response_body=excluded.response_body,
                response_headers=excluded.response_headers,
                response_status=excluded.response_status,
                size=excluded.size",
            params![
                cache_control_str,
                cached.cached_at,
                cached.etag,
                cached.request_hash,
                cached.response_body,
                cached.response_headers,
                cached.response_status,
                cached.size
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_cache::config::HttpCacheConfig;
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
        headers.insert(header_names::ETAG, "\"test-etag\"").unwrap();

        Response {
            request_method: Method::Get,
            url: "https://example.com/test".parse().unwrap(),
            status,
            headers,
            body: body.to_vec(),
        }
    }

    fn create_test_store() -> HttpCacheStore {
        let config = HttpCacheConfigInner::try_from(HttpCacheConfig {
            db_path: "test.db".to_string(),
            max_size_bytes: None,
            ttl_seconds: None,
        })
        .expect("valid test config");
        let initializer = HttpCacheConnectionInitializer {};
        let conn = open_database::open_memory_database(&initializer)
            .expect("failed to open memory cache db");
        HttpCacheStore::new(config, conn)
    }

    #[test]
    fn test_store_and_retrieve() {
        let store = create_test_store();

        let request = create_test_request("https://example.com/api", b"test body");
        let response = create_test_response(200, b"test response");
        let cached = HttpCacheRow::from_request_response(&request, &response);

        store.store(&cached).unwrap();

        let retrieved = store.lookup(&request).unwrap().unwrap();
        assert_eq!(retrieved.response_status, 200);
        assert_eq!(retrieved.response_body, b"test response");
    }

    #[test]
    fn test_ttl_expiration() {
        let config = HttpCacheConfigInner::try_from(HttpCacheConfig {
            db_path: "test.db".to_string(),
            max_size_bytes: None,
            ttl_seconds: Some(1),
        })
        .expect("valid test config");
        let initializer = HttpCacheConnectionInitializer {};
        let conn = open_database::open_memory_database(&initializer)
            .expect("failed to open memory cache db");
        let store = HttpCacheStore::new(config, conn);

        let request = create_test_request("https://example.com/api", b"test body");
        let response = create_test_response(200, b"test response");
        let cached = HttpCacheRow::from_request_response(&request, &response);

        store.store(&cached).unwrap();

        let retrieved = store.lookup(&request).unwrap().unwrap();
        assert_eq!(retrieved.response_body, b"test response");

        std::thread::sleep(Duration::from_secs(2));

        let retrieved_after_expiry = store.lookup(&request).unwrap();
        assert!(retrieved_after_expiry.is_some());
    }

    #[test]
    fn test_max_size_eviction() {
        let config = HttpCacheConfigInner::try_from(HttpCacheConfig {
            db_path: "test.db".to_string(),
            max_size_bytes: Some(1024),
            ttl_seconds: None,
        })
        .expect("valid test config");
        let initializer = HttpCacheConnectionInitializer {};
        let conn = open_database::open_memory_database(&initializer)
            .expect("failed to open memory cache db");
        let store = HttpCacheStore::new(config, conn);

        for i in 0..5 {
            let request = create_test_request(&format!("https://example.com/api/{}", i), b"");
            let large_body = vec![0u8; 300];
            let response = create_test_response(200, &large_body);
            let cached = HttpCacheRow::from_request_response(&request, &response);
            store.store(&cached).unwrap();

            loop {
                let total = store.current_total_size().unwrap();
                if total <= 1024 {
                    break;
                }
                let deleted = store.delete_oldest_entry().unwrap();
                if deleted == 0 {
                    break;
                }
            }
        }

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

        let cached = HttpCacheRow::from_request_response(&request, &response);
        store.store(&cached).unwrap();
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

        let cached = HttpCacheRow::from_request_response(&request, &response);
        store.store(&cached).unwrap();

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
        let cached1 = HttpCacheRow::from_request_response(&request1, &response1);
        store.store(&cached1).unwrap();

        let request2 = create_test_request("https://example.com/api2", b"test body 2");
        let response2 = create_test_response(200, b"test response 2");
        let cached2 = HttpCacheRow::from_request_response(&request2, &response2);
        store.store(&cached2).unwrap();

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

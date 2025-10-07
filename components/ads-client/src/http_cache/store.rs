use crate::http_cache::config::HttpCacheConfigInner;
use crate::http_cache::row::HttpCacheRow;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};
use viaduct::{Request, Response};

pub struct HttpCacheStore {
    config: HttpCacheConfigInner,
    conn: Mutex<Connection>,
}

impl HttpCacheStore {
    pub fn new(config: HttpCacheConfigInner, conn: Connection) -> Self {
        Self {
            config,
            conn: Mutex::new(conn),
        }
    }

    pub fn cleanup_expired(&self) -> SqliteResult<()> {
        let cutoff = HttpCacheRow::now_epoch() - self.config.ttl.as_secs() as i64;
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM http_cache WHERE cached_at < ?1",
            params![cutoff],
        )?;
        Ok(())
    }

    pub fn current_total_size(&self) -> SqliteResult<i64> {
        let conn = self.conn.lock();
        conn.query_row("SELECT COALESCE(SUM(size),0) FROM http_cache", [], |row| {
            row.get(0)
        })
    }

    pub fn lookup(&self, request: &Request) -> SqliteResult<Option<Response>> {
        let request_hash = HttpCacheRow::hash_request(request);
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT cache_control, cached_at, etag, request_hash, response_body, response_headers, response_status, size FROM http_cache WHERE request_hash = ?1",
            params![request_hash],
            |row| {
                let cached = HttpCacheRow::from_row(row)?;
                let ttl = cached.cache_control.get_ttl(self.config.ttl);
                let cutoff = HttpCacheRow::now_epoch() - ttl.as_secs() as i64;

                Ok(if cached.cached_at >= cutoff {
                    Some(cached.to_response(request))
                } else {
                    None
                })
            },
        )
        .optional()
        .map(|result| result.flatten())
    }

    pub fn store(&self, request: &Request, response: &Response) -> SqliteResult<()> {
        let cached = HttpCacheRow::from_request_response(request, response);

        if !cached.cache_control.should_cache() {
            return Ok(());
        }

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

    pub fn trim_to_max_size(&self) -> SqliteResult<()> {
        loop {
            let total = self.current_total_size()?;
            if total <= self.config.max_size.as_u64() as i64 {
                break;
            }

            let conn = self.conn.lock();
            let deleted = conn.execute(
                "DELETE FROM http_cache WHERE rowid IN (
                    SELECT rowid FROM http_cache ORDER BY cached_at ASC LIMIT 1
                )",
                [],
            )?;
            drop(conn);

            if deleted == 0 {
                break;
            }
        }
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
    use viaduct::{header_names, Headers, Method};

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
        let config = HttpCacheConfigInner::from(HttpCacheConfig {
            db_path: None,
            max_size_bytes: None,
            ttl_seconds: None,
        });
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

        store.store(&request, &response).unwrap();

        let retrieved = store.lookup(&request).unwrap().unwrap();
        assert_eq!(retrieved.status, 200);
        assert_eq!(retrieved.body, b"test response");
    }

    #[test]
    fn test_ttl_expiration() {
        let config = HttpCacheConfigInner::from(HttpCacheConfig {
            db_path: None,
            max_size_bytes: None,
            ttl_seconds: Some(1),
        });
        let initializer = HttpCacheConnectionInitializer {};
        let conn = open_database::open_memory_database(&initializer)
            .expect("failed to open memory cache db");
        let store = HttpCacheStore::new(config, conn);

        let request = create_test_request("https://example.com/api", b"test body");
        let response = create_test_response(200, b"test response");

        store.store(&request, &response).unwrap();

        let retrieved = store.lookup(&request).unwrap().unwrap();
        assert_eq!(retrieved.body, b"test response");

        std::thread::sleep(Duration::from_secs(2));

        let retrieved_after_expiry = store.lookup(&request).unwrap();
        assert!(retrieved_after_expiry.is_none());
    }

    #[test]
    fn test_max_size_eviction() {
        let config = HttpCacheConfigInner::from(HttpCacheConfig {
            db_path: None,
            max_size_bytes: Some(1000),
            ttl_seconds: None,
        });
        let initializer = HttpCacheConnectionInitializer {};
        let conn = open_database::open_memory_database(&initializer)
            .expect("failed to open memory cache db");
        let store = HttpCacheStore::new(config, conn);

        for i in 0..5 {
            let request = create_test_request(&format!("https://example.com/api/{}", i), b"");
            let large_body = vec![0u8; 300];
            let response = create_test_response(200, &large_body);
            store.store(&request, &response).unwrap();
            store.trim_to_max_size().unwrap();
        }

        let total_size = store.current_total_size().unwrap();
        assert!(total_size <= 1000);

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
            .insert(header_names::CACHE_CONTROL, "no-store")
            .unwrap();

        // Should not cache due to no-store directive
        store.store(&request, &response).unwrap();
        let retrieved = store.lookup(&request).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_max_age_directive() {
        let store = create_test_store();
        let request = create_test_request("https://example.com/api", b"test body");

        let mut response = create_test_response(200, b"test response");
        response
            .headers
            .insert(header_names::CACHE_CONTROL, "max-age=1")
            .unwrap();

        store.store(&request, &response).unwrap();

        // Should be cached initially
        let retrieved = store.lookup(&request).unwrap();
        assert!(retrieved.is_some());

        // Wait for expiration
        std::thread::sleep(Duration::from_secs(2));

        // Should be expired now
        let retrieved = store.lookup(&request).unwrap();
        assert!(retrieved.is_none());
    }
}

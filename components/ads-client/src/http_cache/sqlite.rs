use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::{
    clock::Clock,
    http_cache::{
        connection_initializer::HttpCacheConnectionInitializer, request_hash::RequestHash,
        store::HttpCacheDb, ByteSize,
    },
};
use rusqlite::{params, OptionalExtension};
use sql_support::{open_database, LazyDb};
use viaduct::{Header, Response};

impl HttpCacheDb for LazyDb<HttpCacheConnectionInitializer> {
    fn clear_all(&self) -> Result<usize, open_database::Error> {
        let (conn, _) = self.lock()?;
        Ok(conn.execute("DELETE FROM http_cache", [])?)
    }

    fn close(&self) {
        self.close(true);
    }

    /// Removes all entries from the store whose expires_at is at or before the current time.
    fn delete_expired_entries(&self, clock: Arc<dyn Clock>) -> Result<usize, open_database::Error> {
        let (conn, _) = self.lock()?;
        Ok(conn.execute(
            "DELETE FROM http_cache WHERE expires_at <= ?1",
            params![clock.now_epoch_seconds()],
        )?)
    }

    fn invalidate_by_hash(
        &self,
        request_hash: &RequestHash,
    ) -> Result<usize, open_database::Error> {
        let (conn, _) = self.lock()?;
        Ok(conn.execute(
            "DELETE FROM http_cache WHERE request_hash = ?1",
            params![request_hash.to_string()],
        )?)
    }

    /// Lookup is agnostic to expiration. If it exists in the store, it will return the result.
    fn lookup(&self, request_hash: &RequestHash) -> Result<Option<Response>, open_database::Error> {
        let (conn, _) = self.lock()?;
        Ok(conn.query_row(
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
        .optional()?)
    }

    fn store_with_ttl(
        &self,
        request_hash: &RequestHash,
        response: &Response,
        ttl: &Duration,
        clock: Arc<dyn Clock>,
    ) -> Result<(), open_database::Error> {
        let headers_map: HashMap<String, String> = response.headers.clone().into();
        let response_headers = serde_json::to_vec(&headers_map).unwrap_or_default();
        let size_bytes = (response_headers.len() + response.body.len()) as i64;
        let now = clock.now_epoch_seconds();
        let ttl_seconds = ttl.as_secs();
        let expires_at = now + ttl_seconds as i64;

        let (conn, _) = self.lock()?;
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

    /// Deletes the oldest entries until the cache fits in `max_size`.
    fn trim_to_max_size(&self, max_size: ByteSize) -> Result<(), open_database::Error> {
        // Keeps the newest rows that fit in `max_size` and deletes the rest.
        let (conn, _) = self.lock()?;
        conn.execute(
            "DELETE FROM http_cache
             WHERE rowid IN (
                 SELECT rowid
                 FROM (
                     SELECT rowid,
                            SUM(size_bytes) OVER (ORDER BY cached_at DESC, rowid DESC) AS kept_bytes
                     FROM http_cache
                 )
                 WHERE kept_bytes > ?1
             )",
            params![max_size.as_u64()],
        )?;
        Ok(())
    }

    fn is_memory(&self) -> Result<bool, open_database::Error> {
        let (conn, _) = self.lock()?;
        // SQLite reports an empty path for in-memory dbs.
        Ok(conn.path() == Some(""))
    }

    /// Returns total size of the cache in bytes.
    #[cfg(test)]
    fn current_total_size_bytes(&self) -> Result<ByteSize, open_database::Error> {
        let (conn, _) = self.lock()?;
        let size_bytes = conn.query_row(
            "SELECT COALESCE(SUM(size_bytes),0) FROM http_cache",
            [],
            |row| row.get(0),
        )?;
        Ok(ByteSize::b(size_bytes))
    }

    #[cfg(test)]
    fn fetch_timestamps(
        &self,
        hash: &RequestHash,
    ) -> Result<(i64, i64, i64), open_database::Error> {
        let (conn, _) = self.lock()?;
        let res = conn.query_row(
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
        )?;
        Ok(res)
    }
}

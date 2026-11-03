use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use parking_lot::Mutex;
use sql_support::open_database;
use url::Url;
use viaduct::Header;

use crate::{
    bytesize::ByteSize,
    http_cache::{store::HttpCacheDb, RequestHash},
    memory_index::MemoryIndex,
};

// A memory-only Rust structural cache that keeps http requests ephemerally in memory.
// Designed for fallback usage if the sqlite database fails to initialize, or invalid parameters are passed.
// The design apes the sqlite `http_cache` table, with two 'indexes' emulated to allow quick expiration of old entries, etc.
pub struct HttpCacheMemory {
    inner: Arc<Mutex<HttpCacheMemoryInner>>,
}

struct HttpCacheMemoryInner {
    // BTreeMap backed 'indexes' that allow easy access to iteration over all stored entries by their respective axes.
    cached_at_index: MemoryIndex<i64, RequestHash>,
    expired_at_index: MemoryIndex<i64, RequestHash>,

    arena: HashMap<RequestHash, HttpCacheEntry>,
    total_size_bytes: u64,
}

// Example entry struct for internal storage, mimicking the sqlite's schema.
#[derive(Clone, Debug)]
pub struct HttpCacheEntry {
    cached_at: i64,
    expires_at: i64,
    #[allow(dead_code)]
    ttl_seconds: u64,
    request_hash: RequestHash,
    request_method: viaduct::Method,
    response_status: u16,
    request_url: Url,
    response_body: Vec<u8>,
    response_headers: HashMap<String, String>,
    size_bytes: u64,
}

impl Default for HttpCacheMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpCacheMemory {
    pub fn new() -> HttpCacheMemory {
        HttpCacheMemory {
            inner: Arc::new(Mutex::new(HttpCacheMemoryInner::new())),
        }
    }

    pub fn add(&self, entry: HttpCacheEntry) {
        let mut inner = self.inner.lock();
        inner
            .cached_at_index
            .add(entry.request_hash.clone(), entry.cached_at);
        inner
            .expired_at_index
            .add(entry.request_hash.clone(), entry.expires_at);

        inner.total_size_bytes = inner.total_size_bytes.saturating_add(entry.size_bytes);
        inner.arena.insert(entry.request_hash.clone(), entry);
    }

    pub fn remove(&self, hash: &RequestHash) -> Option<HttpCacheEntry> {
        let mut inner = self.inner.lock();
        let entry = inner.arena.remove(hash);
        if let Some(ref entry) = entry {
            inner
                .cached_at_index
                .remove(&entry.request_hash.clone(), entry.cached_at);
            inner
                .expired_at_index
                .remove(&entry.request_hash.clone(), entry.expires_at);

            inner.total_size_bytes = inner.total_size_bytes.saturating_sub(entry.size_bytes);
        }
        entry
    }

    // Gets the stored memory value, and returns an owned clone.
    pub fn get(&self, hash: &RequestHash) -> Option<HttpCacheEntry> {
        let inner = self.inner.lock();
        inner.arena.get(hash).cloned()
    }

    pub fn clear(&self) -> usize {
        let mut inner = self.inner.lock();
        let count = inner.arena.drain().len();
        inner.cached_at_index.clear();
        inner.expired_at_index.clear();

        count
    }
}

impl HttpCacheMemoryInner {
    pub fn new() -> HttpCacheMemoryInner {
        HttpCacheMemoryInner {
            cached_at_index: MemoryIndex::new(),
            expired_at_index: MemoryIndex::new(),
            arena: HashMap::new(),
            total_size_bytes: 0,
        }
    }
}

impl HttpCacheDb for HttpCacheMemory {
    fn clear_all(&self) -> Result<usize, sql_support::open_database::Error> {
        Ok(self.clear())
    }

    // Closing a memory struct doesn't mean anything; noop here.
    fn close(&self) {}

    fn current_total_size_bytes(
        &self,
    ) -> Result<crate::bytesize::ByteSize, sql_support::open_database::Error> {
        let inner = self.inner.lock();
        Ok(ByteSize::b(inner.total_size_bytes))
    }

    fn delete_expired_entries(
        &self,
        clock: std::sync::Arc<dyn crate::clock::Clock>,
    ) -> Result<usize, sql_support::open_database::Error> {
        let sec = clock.now_epoch_seconds();
        let delete_list: HashSet<_> = {
            let inner = self.inner.lock();
            inner
                .expired_at_index
                .iter()
                .take_while(|(n, _)| sec > **n)
                .map(|(_, s)| s)
                .cloned()
                .collect()
        };
        let count = delete_list.len();
        for s in delete_list {
            self.remove(&s);
        }
        Ok(count)
    }

    fn delete_oldest_cached_row(&self) -> Result<(), sql_support::open_database::Error> {
        let delete_hash = {
            let inner = self.inner.lock();
            let hash = inner.cached_at_index.iter().next().map(|(_, b)| b);
            hash.cloned()
        };
        if let Some(delete_hash) = delete_hash {
            self.invalidate_by_hash(&delete_hash)?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn fetch_timestamps(
        &self,
        hash: &super::RequestHash,
    ) -> Result<(i64, i64, i64), sql_support::open_database::Error> {
        use sql_support::open_database;

        let entry = self.get(&hash).ok_or(open_database::Error::SqlError(
            rusqlite::Error::QueryReturnedNoRows,
        ))?;
        Ok((entry.cached_at, entry.expires_at, entry.ttl_seconds as i64))
    }

    fn invalidate_by_hash(
        &self,
        request_hash: &super::RequestHash,
    ) -> Result<usize, sql_support::open_database::Error> {
        match self.remove(request_hash) {
            Some(_) => Ok(1),
            None => Ok(0),
        }
    }

    // Clones a cached response as a `viaduct::Response`
    fn lookup(
        &self,
        request_hash: &super::RequestHash,
    ) -> Result<Option<viaduct::Response>, sql_support::open_database::Error> {
        let Some(entry) = self.get(request_hash) else {
            return Ok(None);
        };
        let headers = entry
            .response_headers
            .clone()
            .into_iter()
            .filter_map(|(n, v)| Header::new(n, v).ok())
            .collect::<Vec<_>>()
            .into();

        Ok(Some(viaduct::Response {
            body: entry.response_body.clone(),
            headers,
            request_method: entry.request_method,
            status: entry.response_status,
            url: entry.request_url.clone(),
        }))
    }

    fn store_with_ttl(
        &self,
        request_hash: &super::RequestHash,
        response: &viaduct::Response,
        ttl: &std::time::Duration,
        clock: std::sync::Arc<dyn crate::clock::Clock>,
    ) -> Result<(), sql_support::open_database::Error> {
        let headers_map: HashMap<String, String> = response.headers.clone().into();
        let response_headers = serde_json::to_vec(&headers_map).unwrap_or_default();
        let size_bytes = (response_headers.len() + response.body.len()) as u64;
        let now = clock.now_epoch_seconds();
        let ttl_seconds = ttl.as_secs();
        let expires_at = now + ttl_seconds as i64;

        let entry = HttpCacheEntry {
            request_hash: request_hash.clone(),
            cached_at: now,
            expires_at,
            ttl_seconds,
            request_method: response.request_method,
            response_status: response.status,
            request_url: response.url.clone(),
            response_body: response.body.clone(),
            response_headers: headers_map,
            size_bytes,
        };

        // Add recreated HttpCacheEntry to the memory mapping.
        self.add(entry);
        Ok(())
    }

    fn is_memory(&self) -> Result<bool, open_database::Error> {
        Ok(true)
    }

    fn is_sqlite(&self) -> bool {
        false
    }
}

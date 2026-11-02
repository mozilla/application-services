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
};

// A memory-only Rust structural cache that keeps http requests ephemerally in memory.
// Designed for fallback if the sqlite database fails to initialize, or invalid parameters are passed.
pub struct HttpCacheMemory {
    inner: Arc<Mutex<HttpCacheMemoryInner>>,
}

struct HttpCacheMemoryInner {
    cached_at_index: MemoryIndex<i64, RequestHash>,
    expired_at_index: MemoryIndex<i64, RequestHash>,
    arena: HashMap<RequestHash, HttpCacheEntry>,
    total_size_bytes: u64,
}

// A simple index structure that maintains a sorted list.
// TODO: Overkill?
#[derive(Debug)]
pub struct MemoryIndex<
    IndexKey: PartialOrd + Ord + std::fmt::Debug,
    ArenaKey: PartialOrd + Ord + std::fmt::Debug,
>(Vec<(IndexKey, ArenaKey)>);
impl<
        IndexKey: PartialOrd + Ord + std::fmt::Debug,
        ArenaKey: PartialOrd + Ord + std::fmt::Debug,
    > Default for MemoryIndex<IndexKey, ArenaKey>
{
    fn default() -> Self {
        Self::new()
    }
}

// TODO: simplify this?
impl<
        IndexKey: PartialOrd + Ord + std::fmt::Debug,
        ArenaKey: PartialOrd + Ord + std::fmt::Debug,
    > MemoryIndex<IndexKey, ArenaKey>
{
    pub fn new() -> MemoryIndex<IndexKey, ArenaKey> {
        Self(Vec::new())
    }

    pub fn remove(&mut self, arena_key: &ArenaKey, val: IndexKey) {
        if let Ok(pos) = self
            .0
            .binary_search_by(|(a0, a1)| a0.cmp(&val).then(a1.cmp(arena_key)))
        {
            self.0.remove(pos);
        }
    }

    pub fn add(&mut self, arena_key: ArenaKey, val: IndexKey) {
        let pos = self
            .0
            .binary_search_by(|(a0, a1)| a0.cmp(&val).then(a1.cmp(&arena_key)))
            .unwrap_or_else(|e| e);

        self.0.insert(pos, (val, arena_key));
    }

    pub fn iter(&self) -> std::slice::Iter<'_, (IndexKey, ArenaKey)> {
        self.0.iter()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

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

    // Gets the stored value with a clone. /// TODO: Rewrite that doc
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
        // TODO: could be a function on index?
        let delete_list: HashSet<_> = {
            let inner = self.inner.lock(); // TODO: These locks should be either in httpcachememory or in this implementation...
            inner
                .expired_at_index
                .iter()
                .take_while(|(n, _)| sec > *n)
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
            println!("cached at index: {:?}", inner.cached_at_index);
            inner.cached_at_index.iter().next().map(|(_, b)| b).cloned()
        };
        println!("Deleting: {:?}", delete_hash);
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
        println!("Checking: {request_hash}");
        let Some(entry) = self.get(request_hash) else {
            return Ok(None);
        };
        println!("Exists, continuing");

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
        println!("SToring hash: {:?}", request_hash);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_cache::store::{FaultKind, HttpCacheStore};
    use sql_support::open_database;
    use std::time::Duration;
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

    fn create_test_store() -> HttpCacheStore {
        let db = HttpCacheMemory::new();
        HttpCacheStore::new_with_test_clock(Box::new(db))
    }

    #[test]
    fn test_lookup_fault_injection() {
        let store = create_test_store();
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

    #[test]
    fn test_store_fault_injection() {
        let store = create_test_store();
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

    #[test]
    fn test_trim_fault_injection() {
        let store = create_test_store();
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

    #[test]
    fn test_cleanup_fault_injection() {
        let store = create_test_store();
        store.set_fault(FaultKind::Cleanup);

        let err = store.delete_expired_entries().unwrap_err();
        match err {
            open_database::Error::SqlError(rusqlite::Error::SqliteFailure(_, Some(msg))) => {
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

    #[test]
    fn test_upsert_refreshes_ttl_and_expiry() {
        let store = create_test_store();
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

    #[test]
    fn test_delete_expired_removes_only_expired() {
        let store = create_test_store();
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

        store.clock().advance(2);
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
            .store_with_ttl(&hash, &resp, &Duration::from_secs(1))
            .unwrap();
        store.clock().advance(2);
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
            .store_with_ttl(&hash, &resp, &Duration::from_secs(0))
            .unwrap();
        assert!(store.lookup(&hash).unwrap().is_some());

        store.clock().advance(2);
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
            .store_with_ttl(&hash, &response, &Duration::from_secs(300))
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
            .store_with_ttl(&hash, &response, &Duration::from_secs(300))
            .unwrap();

        let retrieved = store.lookup(&hash).unwrap().unwrap();
        assert_eq!(retrieved.body, b"test response");

        store.clock().advance(2);

        let retrieved_after_expiry = store.lookup(&hash).unwrap();
        assert!(retrieved_after_expiry.is_some());
    }

    #[test]
    fn test_max_size_eviction() {
        let store = create_test_store();

        for i in 0..5 {
            let request = create_test_request(&format!("https://example.com/api/{}", i), b"");
            let hash = hash_for_request(&request);
            println!("Hash {i}: {hash}");
            let large_body = vec![0u8; 300];
            let response = create_test_response(200, &large_body);
            store
                .store_with_ttl(&hash, &response, &Duration::from_secs(300))
                .unwrap();

            // TODO: Divergent behaviourt here requires this.

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
            .store_with_ttl(&hash, &response, &Duration::from_secs(300))
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

    #[test]
    fn test_invalidate_by_hash() {
        let store = create_test_store();

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

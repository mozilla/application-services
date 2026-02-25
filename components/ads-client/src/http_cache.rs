/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod builder;
mod bytesize;
mod cache_control;
mod clock;
mod connection_initializer;
mod request_hash;
mod store;

use self::{builder::HttpCacheBuilder, cache_control::CacheControl, store::HttpCacheStore};

use std::hash::Hash;
use viaduct::{Request, Response};

pub use self::builder::HttpCacheBuilderError;
pub use self::bytesize::ByteSize;
pub use self::request_hash::RequestHash;
use std::cmp;
use std::path::Path;
use std::time::Duration;

pub type HttpCacheSendResult<T> = std::result::Result<T, viaduct::ViaductError>;

#[derive(Clone, Copy, Debug, Default)]
pub struct RequestCachePolicy {
    pub mode: CacheMode,
    pub ttl_seconds: Option<u64>, // optional client-defined ttl override
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CacheMode {
    #[default]
    CacheFirst,
    NetworkFirst,
}

#[derive(Debug, thiserror::Error)]
pub enum HttpCacheError {
    #[error("Could not build cache: {0}")]
    Builder(#[from] builder::HttpCacheBuilderError),

    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug)]
pub enum CacheOutcome {
    Hit,
    LookupFailed(rusqlite::Error), // cache miss path due to lookup error
    NoCache,                       // send policy requested a cache bypass
    MissNotCacheable,              // policy says "don't store"
    MissStored,                    // stored successfully
    StoreFailed(HttpCacheError),   // insert/upsert failed
    CleanupFailed(HttpCacheError), // cleaning expired objects failed
}

pub struct SendOutcome {
    pub response: Response,
    pub cache_outcome: CacheOutcome,
}

pub struct HttpCache<T: Hash + Into<Request>> {
    max_size: ByteSize,
    store: HttpCacheStore,
    default_ttl: Duration,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Hash + Into<Request>> HttpCache<T> {
    pub fn builder<P: AsRef<Path>>(db_path: P) -> HttpCacheBuilder<T> {
        HttpCacheBuilder::new(db_path.as_ref())
    }

    pub fn clear(&self) -> Result<(), HttpCacheError> {
        self.store.clear_all().map_err(HttpCacheError::from)?;
        Ok(())
    }

    pub fn invalidate_by_hash(&self, request_hash: &RequestHash) -> Result<(), HttpCacheError> {
        self.store
            .invalidate_by_hash(request_hash)
            .map_err(HttpCacheError::from)?;
        Ok(())
    }

    pub fn send_with_policy(
        &self,
        item: T,
        request_policy: &RequestCachePolicy,
    ) -> HttpCacheSendResult<SendOutcome> {
        let request_hash = RequestHash::new(&item);
        let request: Request = item.into();
        let request_policy_ttl = match request_policy.ttl_seconds {
            Some(s) => Duration::new(s, 0),
            None => self.default_ttl,
        };

        if request_policy.mode == CacheMode::CacheFirst {
            match self.store.lookup(&request_hash) {
                Ok(Some(response)) => {
                    return Ok(SendOutcome {
                        response,
                        cache_outcome: CacheOutcome::Hit,
                    });
                }
                Err(e) => {
                    let mut outcome =
                        self.fetch_and_cache(&request, &request_hash, &request_policy_ttl)?;
                    outcome.cache_outcome = CacheOutcome::LookupFailed(e);
                    return Ok(outcome);
                }
                Ok(None) => {}
            }
        }

        self.fetch_and_cache(&request, &request_hash, &request_policy_ttl)
    }

    fn fetch_and_cache(
        &self,
        request: &Request,
        request_hash: &RequestHash,
        request_policy_ttl: &Duration,
    ) -> HttpCacheSendResult<SendOutcome> {
        let response = request.clone().send()?;
        if let Err(e) = self.store.delete_expired_entries() {
            return Ok(SendOutcome {
                response,
                cache_outcome: CacheOutcome::CleanupFailed(e.into()),
            });
        }
        let cache_control = CacheControl::from(&response);
        let cache_outcome = if cache_control.should_cache() {
            let response_ttl = match cache_control.max_age {
                Some(s) => Duration::new(s, 0),
                None => self.default_ttl,
            };

            // We respect the smallest ttl between the policy, default client value, or header
            let final_ttl = cmp::min(
                cmp::min(*request_policy_ttl, self.default_ttl),
                response_ttl,
            );

            if final_ttl.as_secs() == 0 {
                return Ok(SendOutcome {
                    response,
                    cache_outcome: CacheOutcome::NoCache,
                });
            }

            match self.cache_object(request_hash, &response, &final_ttl) {
                Ok(()) => CacheOutcome::MissStored,
                Err(e) => CacheOutcome::StoreFailed(e),
            }
        } else {
            CacheOutcome::MissNotCacheable
        };

        Ok(SendOutcome {
            response,
            cache_outcome,
        })
    }

    fn cache_object(
        &self,
        request_hash: &RequestHash,
        response: &Response,
        ttl: &Duration,
    ) -> Result<(), HttpCacheError> {
        self.store.store_with_ttl(request_hash, response, ttl)?;
        self.store.trim_to_max_size(self.max_size.as_u64() as i64)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use mockito::mock;
    use std::hash::{Hash, Hasher};

    use super::*;

    /// Test-only hashable wrapper around Request.
    /// Hashes method + url for cache key purposes.
    #[derive(Clone)]
    struct TestRequest(Request);

    impl Hash for TestRequest {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.0.method.as_str().hash(state);
            self.0.url.as_str().hash(state);
        }
    }

    impl From<TestRequest> for Request {
        fn from(t: TestRequest) -> Self {
            t.0
        }
    }

    fn make_post_request() -> TestRequest {
        let url = format!("{}/ads", mockito::server_url()).parse().unwrap();
        TestRequest(Request::post(url).json(&serde_json::json!({"fake":"data"})))
    }

    fn make_cache() -> HttpCache<TestRequest> {
        // Our store opens an in-memory cache for tests. So the name is irrelevant.
        HttpCache::builder("ignored_in_tests.db")
            .default_ttl(Duration::from_secs(60))
            .max_size(ByteSize::mib(1))
            .build()
            .expect("cache build should succeed")
    }

    fn make_cache_with_ttl(secs: u64) -> HttpCache<TestRequest> {
        // In tests our store uses an in-memory DB; filename is irrelevant.
        HttpCache::builder("ignored_in_tests.db")
            .default_ttl(Duration::from_secs(secs))
            .max_size(ByteSize::mib(4))
            .build_for_time_dependent_tests()
            .expect("cache build should succeed")
    }

    #[test]
    fn test_http_cache_creation() {
        // Test that HttpCache can be created successfully with test config
        let cache: Result<HttpCache<TestRequest>, _> = HttpCache::builder("test_cache.db").build();
        assert!(cache.is_ok());

        // Test with custom config
        let cache_with_config: Result<HttpCache<TestRequest>, _> =
            HttpCache::builder("custom_test.db")
                .max_size(ByteSize::mib(1))
                .default_ttl(Duration::from_secs(60))
                .build();
        assert!(cache_with_config.is_ok());
    }

    #[test]
    fn test_clear_cache() {
        let cache: HttpCache<TestRequest> = HttpCache::builder("test_clear.db").build().unwrap();

        // Create a test request and response
        let hash = RequestHash::new(&("Get", "https://example.com/test"));

        let response = viaduct::Response {
            request_method: viaduct::Method::Get,
            url: "https://example.com/test".parse().unwrap(),
            status: 200,
            headers: viaduct::Headers::new(),
            body: b"test response".to_vec(),
        };

        cache
            .store
            .store_with_ttl(&hash, &response, &Duration::new(300, 0))
            .unwrap();

        // Verify it's cached
        let retrieved = cache.store.lookup(&hash).unwrap();
        assert!(retrieved.is_some());

        // Clear the cache
        cache.clear().unwrap();

        // Verify it's cleared
        let retrieved_after_clear = cache.store.lookup(&hash).unwrap();
        assert!(retrieved_after_clear.is_none());
    }

    #[test]
    fn test_default_policy_miss_then_store_then_hit() {
        viaduct_dev::init_backend_dev();

        let body = r#"{"ok":true}"#;
        let _m = mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .expect(1) // only the first call should hit the network
            .create();

        let cache = make_cache();
        let req = make_post_request();

        // First call: miss -> store
        let o1 = cache
            .send_with_policy(req.clone(), &RequestCachePolicy::default())
            .unwrap();
        matches!(o1.cache_outcome, CacheOutcome::MissStored);

        // Second call: hit (no extra HTTP request due to expect(1))
        let o2 = cache
            .send_with_policy(req, &RequestCachePolicy::default())
            .unwrap();
        matches!(o2.cache_outcome, CacheOutcome::Hit);
        assert_eq!(o2.response.status, 200);
    }

    #[test]
    fn test_refresh_policy_always_uses_network_then_caches() {
        viaduct_dev::init_backend_dev();

        let body1 = r#"{"ok":true,"n":1}"#;
        let body2 = r#"{"ok":true,"n":2}"#;
        // Two live responses expected on refresh
        let _m1 = mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body1)
            .create();
        let _m2 = mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body2)
            .create();

        let cache = make_cache();
        let req = make_post_request();

        // First refresh: live -> MissStored
        let o1 = cache
            .send_with_policy(
                req.clone(),
                &RequestCachePolicy {
                    mode: CacheMode::NetworkFirst,
                    ttl_seconds: None,
                },
            )
            .unwrap();
        matches!(o1.cache_outcome, CacheOutcome::MissStored);

        // Second refresh: live again (different body), still MissStored
        let o2 = cache
            .send_with_policy(
                req,
                &RequestCachePolicy {
                    mode: CacheMode::NetworkFirst,
                    ttl_seconds: None,
                },
            )
            .unwrap();
        matches!(o2.cache_outcome, CacheOutcome::MissStored);
        assert_eq!(o2.response.status, 200);
    }

    #[test]
    fn test_not_cacheable_no_store() {
        viaduct_dev::init_backend_dev();

        let _m = mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("cache-control", "no-store") // should block caching
            .with_body(r#"{"ok":true}"#)
            .expect(1)
            .create();

        let cache = make_cache();
        let req = make_post_request();

        let o = cache
            .send_with_policy(req.clone(), &RequestCachePolicy::default())
            .unwrap();
        matches!(o.cache_outcome, CacheOutcome::MissNotCacheable);

        // Next call should hit network again (since we didn't cache)
        let _m2 = mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok":true}"#)
            .expect(1)
            .create();
        let o2 = cache
            .send_with_policy(req, &RequestCachePolicy::default())
            .unwrap();
        // Either MissStored (if headers differ) or MissNotCacheable if still no-store
        assert!(matches!(
            o2.cache_outcome,
            CacheOutcome::MissStored | CacheOutcome::MissNotCacheable
        ));
    }

    #[test]
    fn ttl_resolution_min_of_server_request_default() {
        viaduct_dev::init_backend_dev();

        let _m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("cache-control", "max-age=1") // Set max age to 1 second
            .with_body(r#"{"ok":true}"#)
            .expect(1)
            .create();

        let cache = make_cache_with_ttl(300);
        let req = make_post_request();
        let hash = RequestHash::new(&req);
        let policy = RequestCachePolicy {
            mode: CacheMode::CacheFirst,
            ttl_seconds: Some(20), // 20 second ttl specified vs the cache's default of 300s
        };

        // Store ttl should resolve to 1s as specified by response headers
        let out = cache.send_with_policy(req, &policy).unwrap();
        assert!(matches!(out.cache_outcome, CacheOutcome::MissStored));

        // After ~>1s, cleanup should remove it
        cache.store.get_clock().advance(2);
        cache.store.delete_expired_entries().unwrap();

        assert!(cache.store.lookup(&hash).unwrap().is_none());
    }

    #[test]
    fn ttl_resolution_request_overrides_default_when_smaller() {
        viaduct_dev::init_backend_dev();

        let _m = mockito::mock("POST", "/ads")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok":true}"#)
            .expect(1)
            .create();

        let cache = make_cache_with_ttl(60);
        let req = make_post_request();
        let hash = RequestHash::new(&req);
        let policy = RequestCachePolicy {
            mode: CacheMode::CacheFirst,
            ttl_seconds: Some(2),
        };

        // Store with effective TTL = 2s
        let out = cache.send_with_policy(req, &policy).unwrap();
        assert!(matches!(out.cache_outcome, CacheOutcome::MissStored));

        // Not expired yet at ~1s
        cache.store.get_clock().advance(1);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup(&hash).unwrap().is_some());

        // Expired after ~2s
        cache.store.get_clock().advance(2);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup(&hash).unwrap().is_none());
    }

    #[test]
    fn ttl_resolution_uses_default_when_no_server_and_no_request_override() {
        viaduct_dev::init_backend_dev();

        let _m = mockito::mock("POST", "/ads")
            .with_status(200)
            // No response policy ttl
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok":true}"#)
            .expect(1)
            .create();

        let cache = make_cache_with_ttl(2);
        let req = make_post_request();
        let hash = RequestHash::new(&req);
        // No request policy ttl
        let policy = RequestCachePolicy::default();

        // Store with effective TTL = 2s from client default
        let out = cache.send_with_policy(req, &policy).unwrap();
        assert!(matches!(out.cache_outcome, CacheOutcome::MissStored));

        // Not expired at ~1s
        cache.store.get_clock().advance(1);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup(&hash).unwrap().is_some());

        // Expired after ~3s
        cache.store.get_clock().advance(3);
        cache.store.delete_expired_entries().unwrap();
        assert!(cache.store.lookup(&hash).unwrap().is_none());
    }

    #[test]
    fn test_invalidate_by_hash() {
        let cache: HttpCache<TestRequest> =
            HttpCache::builder("test_invalidate.db").build().unwrap();

        let hash1 = RequestHash::new(&("Post", "https://example.com/api1"));
        let hash2 = RequestHash::new(&("Post", "https://example.com/api2"));

        let response = viaduct::Response {
            request_method: viaduct::Method::Post,
            url: "https://example.com/test".parse().unwrap(),
            status: 200,
            headers: viaduct::Headers::new(),
            body: b"test response".to_vec(),
        };

        cache
            .store
            .store_with_ttl(&hash1, &response, &Duration::new(300, 0))
            .unwrap();

        cache
            .store
            .store_with_ttl(&hash2, &response, &Duration::new(300, 0))
            .unwrap();

        assert!(cache.store.lookup(&hash1).unwrap().is_some());
        assert!(cache.store.lookup(&hash2).unwrap().is_some());

        cache.invalidate_by_hash(&hash1).unwrap();

        assert!(cache.store.lookup(&hash1).unwrap().is_none());
        assert!(cache.store.lookup(&hash2).unwrap().is_some());
    }
}

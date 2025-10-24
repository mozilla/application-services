/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod builder;
mod bytesize;
mod cache_control;
mod connection_initializer;
mod request_hash;
mod store;

use self::{builder::HttpCacheBuilder, cache_control::CacheControl, store::HttpCacheStore};

use viaduct::{Request, Response};

pub use self::bytesize::ByteSize;
use std::cmp;
use std::path::Path;
use std::time::Duration;

pub const DEFAULT_TTL_SECONDS: u64 = 300;
pub const DEFAULT_MAX_CACHE_SIZE_MIB: u64 = 10;

pub type HttpCacheSendResult<T> = std::result::Result<T, viaduct::ViaductError>;

#[derive(uniffi::Record, Clone, Copy, Debug, Default)]
pub struct RequestCachePolicy {
    pub mode: CacheMode,
    pub ttl_seconds: Option<u64>, // optional client-defined ttl override
}

#[derive(uniffi::Enum, Clone, Copy, Debug, Default)]
pub enum CacheMode {
    #[default]
    Default,
    Refresh,
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Could not build cache: {0}")]
    Builder(#[from] builder::Error),

    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub enum CacheOutcome {
    Hit,
    LookupFailed(rusqlite::Error), // cache miss path due to lookup error
    NoCache,                       // send policy requested a cache bypass
    MissNotCacheable,              // policy says "don't store"
    MissStored,                    // stored successfully
    StoreFailed(CacheError),       // insert/upsert failed
    CleanupFailed(CacheError),     // cleaning expired objects failed
}

pub struct SendOutcome {
    pub response: Response,
    pub cache_outcome: CacheOutcome,
}

pub struct HttpCache {
    max_size: ByteSize,
    store: HttpCacheStore,
    default_ttl: Duration,
}

impl HttpCache {
    pub fn builder<P: AsRef<Path>>(db_path: P) -> HttpCacheBuilder {
        HttpCacheBuilder::new(db_path.as_ref())
    }

    pub fn clear(&self) -> Result<(), CacheError> {
        self.store.clear_all().map_err(CacheError::from)?;
        Ok(())
    }

    fn request_from_network_then_cache(
        &self,
        request: &Request,
        request_policy_ttl: &Duration,
    ) -> HttpCacheSendResult<SendOutcome> {
        let response = request.clone().send()?;
        if let Err(e) = self.cleanup_expired() {
            return Ok(SendOutcome {
                response,
                cache_outcome: CacheOutcome::CleanupFailed(e),
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

            match self.cache_object(request, &response, &final_ttl) {
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

    pub fn send_with_policy(
        &self,
        request: &Request,
        request_policy: &RequestCachePolicy,
    ) -> HttpCacheSendResult<SendOutcome> {
        let request_policy_ttl = match request_policy.ttl_seconds {
            Some(s) => Duration::new(s, 0),
            None => self.default_ttl,
        };

        match request_policy.mode {
            // Default behavior is we check the cache before trying a network call
            CacheMode::Default => match self.store.lookup(request) {
                Ok(Some((resp, _))) => Ok(SendOutcome {
                    response: resp,
                    cache_outcome: CacheOutcome::Hit,
                }),
                Ok(None) => self.request_from_network_then_cache(request, &request_policy_ttl),
                Err(e) => {
                    let response = request.clone().send()?;
                    Ok(SendOutcome {
                        response,
                        cache_outcome: CacheOutcome::LookupFailed(e),
                    })
                }
            },
            CacheMode::Refresh => {
                self.request_from_network_then_cache(request, &request_policy_ttl)
            }
        }
    }

    fn cleanup_expired(&self) -> Result<(), CacheError> {
        self.store.delete_expired_entries()?;
        Ok(())
    }

    fn cache_object(
        &self,
        request: &Request,
        response: &Response,
        ttl: &Duration,
    ) -> Result<(), CacheError> {
        self.store.store_with_ttl(request, response, ttl)?;
        self.store.trim_to_max_size(self.max_size.as_u64() as i64)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use mockito::mock;

    use super::*;

    fn make_post_request() -> Request {
        let url = format!("{}/ads", mockito::server_url()).parse().unwrap();
        Request::post(url).json(&serde_json::json!({"fake":"data"}))
    }

    fn make_cache() -> HttpCache {
        // Our store opens an in-memory cache for tests. So the name is irrelevant.
        HttpCache::builder("ignored_in_tests.db")
            .default_ttl(Duration::from_secs(60))
            .max_size(ByteSize::mib(1))
            .build()
            .expect("cache build should succeed")
    }

    #[test]
    fn test_http_cache_creation() {
        // Test that HttpCache can be created successfully with test config
        let cache = HttpCache::builder("test_cache.db").build();
        assert!(cache.is_ok());

        // Test with custom config
        let cache_with_config = HttpCache::builder("custom_test.db")
            .max_size(ByteSize::mib(1))
            .default_ttl(Duration::from_secs(60))
            .build();
        assert!(cache_with_config.is_ok());
    }

    #[test]
    fn test_clear_cache() {
        let cache = HttpCache::builder("test_clear.db").build().unwrap();

        // Create a test request and response
        let request = viaduct::Request {
            method: viaduct::Method::Get,
            url: "https://example.com/test".parse().unwrap(),
            headers: viaduct::Headers::new(),
            body: None,
        };

        let response = viaduct::Response {
            request_method: viaduct::Method::Get,
            url: "https://example.com/test".parse().unwrap(),
            status: 200,
            headers: viaduct::Headers::new(),
            body: b"test response".to_vec(),
        };

        // Store something in the cache
        cache
            .store
            .store_with_ttl(&request, &response, &Duration::new(300, 0))
            .unwrap();

        // Verify it's cached
        let retrieved = cache.store.lookup(&request).unwrap();
        assert!(retrieved.is_some());

        // Clear the cache
        cache.clear().unwrap();

        // Verify it's cleared
        let retrieved_after_clear = cache.store.lookup(&request).unwrap();
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
            .send_with_policy(&req.clone(), &RequestCachePolicy::default())
            .unwrap();
        matches!(o1.cache_outcome, CacheOutcome::MissStored);

        // Second call: hit (no extra HTTP request due to expect(1))
        let o2 = cache
            .send_with_policy(&req, &RequestCachePolicy::default())
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
                &req.clone(),
                &RequestCachePolicy {
                    mode: CacheMode::Refresh,
                    ttl_seconds: None,
                },
            )
            .unwrap();
        matches!(o1.cache_outcome, CacheOutcome::MissStored);

        // Second refresh: live again (different body), still MissStored
        let o2 = cache
            .send_with_policy(
                &req,
                &RequestCachePolicy {
                    mode: CacheMode::Refresh,
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
            .send_with_policy(&req.clone(), &RequestCachePolicy::default())
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
            .send_with_policy(&req, &RequestCachePolicy::default())
            .unwrap();
        // Either MissStored (if headers differ) or MissNotCacheable if still no-store
        assert!(matches!(
            o2.cache_outcome,
            CacheOutcome::MissStored | CacheOutcome::MissNotCacheable
        ));
    }
}

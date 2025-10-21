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
use std::path::Path;
use std::time::Duration;

pub const DEFAULT_TTL_SECONDS: u64 = 300;
pub const DEFAULT_MAX_CACHE_SIZE_MIB: u64 = 10;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Could not build cache: {0}")]
    Builder(#[from] builder::Error),

    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Error sending request: {0}")]
    Viaduct(#[from] viaduct::ViaductError),
}

pub enum CacheOutcome {
    Hit,
    Skipped,
    MissStoredSuccess,
    MissStoreFailed(CacheError),
    MissNotCacheable,
    FatalError,
}

pub struct SendOutcome {
    pub response: Response,
    pub cache_outcome: CacheOutcome,
}

pub struct HttpCache {
    max_size: ByteSize,
    store: HttpCacheStore,
    ttl: Duration,
}

impl HttpCache {
    pub fn builder<P: AsRef<Path>>(db_path: P) -> HttpCacheBuilder {
        HttpCacheBuilder::new(db_path.as_ref())
    }

    pub fn clear(&self) -> Result<(), CacheError> {
        self.store.clear_all().map_err(CacheError::from)?;
        Ok(())
    }

    pub fn send_with_refresh(&self, request: Request) -> Result<SendOutcome, CacheError> {
        self.cleanup_expired()?;

        let response = request.clone().send()?;
        let cache_outcome = if CacheControl::from(&response).should_cache() {
            match self.cache_object(&request, &response) {
                Ok(()) => CacheOutcome::MissStoredSuccess,
                Err(e) => CacheOutcome::MissStoreFailed(e),
            }
        } else {
            CacheOutcome::MissNotCacheable
        };

        Ok(SendOutcome {
            response,
            cache_outcome,
        })
    }

    pub fn send(&self, request: Request) -> Result<SendOutcome, CacheError> {
        self.cleanup_expired()?;

        if let Some((resp, _)) = self.store.lookup(&request)? {
            return Ok(SendOutcome {
                response: resp,
                cache_outcome: CacheOutcome::Hit,
            });
        }

        let response = request.clone().send()?;
        let cache_outcome = if CacheControl::from(&response).should_cache() {
            match self.cache_object(&request, &response) {
                Ok(()) => CacheOutcome::MissStoredSuccess,
                Err(e) => CacheOutcome::MissStoreFailed(e),
            }
        } else {
            CacheOutcome::MissNotCacheable
        };

        Ok(SendOutcome {
            response,
            cache_outcome,
        })
    }

    fn cleanup_expired(&self) -> Result<(), CacheError> {
        let cutoff = chrono::Utc::now().timestamp() - self.ttl.as_secs() as i64;
        self.store.delete_expired_entries(cutoff)?;
        Ok(())
    }

    fn cache_object(&self, request: &Request, response: &Response) -> Result<(), CacheError> {
        self.store.store(&request, &response)?;
        self.store.trim_to_max_size(self.max_size.as_u64() as i64)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_cache_creation() {
        // Test that HttpCache can be created successfully with test config
        let cache = HttpCache::builder("test_cache.db").build();
        assert!(cache.is_ok());

        // Test with custom config
        let cache_with_config = HttpCache::builder("custom_test.db")
            .max_size(ByteSize::mib(1))
            .ttl(Duration::from_secs(60))
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
        cache.store.store(&request, &response).unwrap();

        // Verify it's cached
        let retrieved = cache.store.lookup(&request).unwrap();
        assert!(retrieved.is_some());

        // Clear the cache
        cache.clear().unwrap();

        // Verify it's cleared
        let retrieved_after_clear = cache.store.lookup(&request).unwrap();
        assert!(retrieved_after_clear.is_none());
    }
}

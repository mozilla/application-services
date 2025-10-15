/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod builder;
mod bytesize;
mod cache_control;
mod connection_initializer;
mod request_hash;
mod store;

use self::{
    builder::HttpCacheBuilder, cache_control::CacheControl, request_hash::RequestHash,
    store::HttpCacheStore,
};

use viaduct::{Request, Response};

pub use self::bytesize::ByteSize;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Could not build cache: {0}")]
    Builder(#[from] builder::Error),

    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Error sending request: {0}")]
    Viaduct(#[from] viaduct::ViaductError),
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

    pub fn clear(&self) -> Result<(), Error> {
        self.store.clear_all().map_err(Error::from)?;
        Ok(())
    }

    pub fn send(&self, request: Request) -> Result<(Response, Option<RequestHash>), Error> {
        let cutoff = chrono::Utc::now().timestamp() - self.ttl.as_secs() as i64;
        self.store.delete_expired_entries(cutoff)?;

        if let Some((response, request_hash)) = self.store.lookup(&request)? {
            return Ok((response, Some(request_hash)));
        }

        let response = request.clone().send()?;
        let cache_control = CacheControl::from(&response);
        if cache_control.should_cache() {
            self.store.store(&request, &response)?;
            self.store.trim_to_max_size(self.max_size.as_u64() as i64)?;
        }
        Ok((response, None))
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

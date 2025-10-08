/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod bytesize;
mod cache_control_directives;
mod config;
mod connection_initializer;
mod row;
mod store;

use self::{
    config::HttpCacheConfigInner, connection_initializer::HttpCacheConnectionInitializer,
    row::HttpCacheRow, store::HttpCacheStore,
};

pub use config::{HttpCacheConfig, HttpCacheConfigError};
use sql_support::open_database;
use std::sync::Arc;
use viaduct::{Request, Response, Result};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum HttpCacheError {
    #[error("Could not open SQLite database: {0}")]
    OpenDatabase(String),

    #[error("Invalid HTTP cache configuration: {0}")]
    InvalidConfig(#[from] HttpCacheConfigError),
}

#[derive(Debug, thiserror::Error)]
pub enum CacheOperation {
    #[error("Failed to cleanup expired entries")]
    CleanupExpired,
    #[error("Failed to store response in cache")]
    Store,
    #[error("Failed to trim cache to max size")]
    TrimToMaxSize,
}

pub trait CacheFailureCallback: Send + Sync {
    fn on_cache_failure(&self, operation: CacheOperation, error: HttpCacheError);
}

#[derive(uniffi::Object)]
pub struct HttpCache {
    store: HttpCacheStore,
    config: HttpCacheConfigInner,
    failure_callback: Option<Arc<dyn CacheFailureCallback>>,
}

#[uniffi::export]
impl HttpCache {
    #[uniffi::constructor]
    pub fn new(config: HttpCacheConfig) -> Result<Self, HttpCacheError> {
        let internal_config = HttpCacheConfigInner::try_from(config)?;
        let initializer = HttpCacheConnectionInitializer {};
        let conn = if cfg!(test) {
            open_database::open_memory_database(&initializer)
                .map_err(|e| HttpCacheError::OpenDatabase(e.to_string()))?
        } else {
            open_database::open_database(&internal_config.db_path, &initializer)
                .map_err(|e| HttpCacheError::OpenDatabase(e.to_string()))?
        };
        let store = HttpCacheStore::new(internal_config.clone(), conn);
        Ok(Self {
            store,
            config: internal_config,
            failure_callback: None,
        })
    }

    pub fn clear(&self) -> Result<(), HttpCacheError> {
        self.store
            .clear_all()
            .map_err(|e| HttpCacheError::OpenDatabase(e.to_string()))?;
        Ok(())
    }
}

impl HttpCache {
    pub fn send(&self, request: Request) -> Result<Response> {
        if let Err(e) = self.cleanup_expired() {
            self.notify_cache_failure(CacheOperation::CleanupExpired, e);
        }

        if let Some(cached_row) = self.store.lookup(&request).ok().flatten() {
            let ttl = cached_row.cache_control.get_ttl(self.config.ttl);
            let cutoff = HttpCacheRow::now_epoch() - ttl.as_secs() as i64;

            if cached_row.cached_at >= cutoff {
                return Ok(cached_row.to_response(&request));
            }
        }

        let response = request.clone().send()?;
        if response.is_success() {
            let cached = HttpCacheRow::from_request_response(&request, &response);
            if cached.cache_control.should_cache() {
                if let Err(e) = self.store.store(&cached) {
                    self.notify_cache_failure(
                        CacheOperation::Store,
                        HttpCacheError::OpenDatabase(e.to_string()),
                    );
                }
                if let Err(e) = self.trim_to_max_size() {
                    self.notify_cache_failure(CacheOperation::TrimToMaxSize, e);
                }
            }
        }
        Ok(response)
    }

    fn notify_cache_failure(&self, operation: CacheOperation, error: HttpCacheError) {
        if let Some(callback) = &self.failure_callback {
            callback.on_cache_failure(operation, error);
        }
    }

    pub fn set_failure_callback(&mut self, callback: Arc<dyn CacheFailureCallback>) {
        self.failure_callback = Some(callback);
    }

    fn cleanup_expired(&self) -> Result<(), HttpCacheError> {
        let cutoff = HttpCacheRow::now_epoch() - self.config.ttl.as_secs() as i64;
        self.store
            .delete_expired_entries(cutoff)
            .map_err(|e| HttpCacheError::OpenDatabase(e.to_string()))?;
        Ok(())
    }

    fn trim_to_max_size(&self) -> Result<(), HttpCacheError> {
        loop {
            let total = self
                .store
                .current_total_size()
                .map_err(|e| HttpCacheError::OpenDatabase(e.to_string()))?;

            if total <= self.config.max_size.as_u64() as i64 {
                break;
            }

            let deleted = self
                .store
                .delete_oldest_entry()
                .map_err(|e| HttpCacheError::OpenDatabase(e.to_string()))?;

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

    #[test]
    fn test_http_cache_creation() {
        // Test that HttpCache can be created successfully with test config
        let config = HttpCacheConfig {
            db_path: "test_cache.db".to_string(),
            max_size_bytes: None,
            ttl_seconds: None,
        };
        let cache = HttpCache::new(config);
        assert!(cache.is_ok());

        // Test with custom config
        let custom_config = HttpCacheConfig {
            db_path: "custom_test.db".to_string(),
            max_size_bytes: Some(1024),
            ttl_seconds: Some(60),
        };
        let cache_with_config = HttpCache::new(custom_config);
        assert!(cache_with_config.is_ok());
    }

    #[test]
    fn test_clear_cache() {
        let config = HttpCacheConfig {
            db_path: "test_clear.db".to_string(),
            max_size_bytes: None,
            ttl_seconds: None,
        };
        let cache = HttpCache::new(config).unwrap();

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
        let cached = HttpCacheRow::from_request_response(&request, &response);
        cache.store.store(&cached).unwrap();

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
    fn test_cache_failure_callback() {
        use std::sync::{Arc, Mutex};

        struct TestCallback {
            failures: Arc<Mutex<Vec<(CacheOperation, HttpCacheError)>>>,
        }

        impl CacheFailureCallback for TestCallback {
            fn on_cache_failure(&self, operation: CacheOperation, error: HttpCacheError) {
                self.failures.lock().unwrap().push((operation, error));
            }
        }

        let failures = Arc::new(Mutex::new(Vec::new()));
        let callback = Arc::new(TestCallback {
            failures: failures.clone(),
        });

        let config = HttpCacheConfig {
            db_path: "test_callback.db".to_string(),
            max_size_bytes: None,
            ttl_seconds: None,
        };
        let mut cache = HttpCache::new(config).unwrap();
        cache.set_failure_callback(callback);

        // Test that the callback is set (we can't easily test actual failures in unit tests
        // without mocking the database, but we can verify the callback mechanism is in place)
        assert!(cache.failure_callback.is_some());
    }
}

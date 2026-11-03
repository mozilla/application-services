/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::connection_initializer::HttpCacheConnectionInitializer;
use super::store::HttpCacheStore;
use crate::bytesize::ByteSize;
use crate::http_cache::memory::HttpCacheMemory;
use crate::http_cache::HttpCache;
use crate::telemetry;
use crate::IN_MEMORY_DB_PATH;
use rusqlite::OpenFlags;
use sql_support::{open_database, LazyDb};
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_MAX_SIZE: ByteSize = ByteSize::mib(10);
const DEFAULT_TTL: Duration = Duration::from_secs(300);

const MIN_CACHE_SIZE: ByteSize = ByteSize::kib(1);
const MAX_CACHE_SIZE: ByteSize = ByteSize::mib(100);
const MIN_TTL: Duration = Duration::from_secs(1);
const MAX_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 7); // 7 days

#[derive(Debug, thiserror::Error)]
pub enum HttpCacheBuilderError {
    #[error("Database error: {0}")]
    Database(#[from] open_database::Error),
    // TODO: Remove this once memory dbs confirmed to be OK.
    #[error("Database (memory) error: {0}")]
    DatabaseMemory(open_database::Error),
    #[error("Database path cannot be empty")]
    EmptyDbPath,
    #[error(
        "Maximum cache size must be between {min_size} and {max_size}, got {size_bytes} bytes"
    )]
    InvalidMaxSize {
        max_size: String,
        min_size: String,
        size_bytes: u64,
    },
    #[error("TTL must be between {min_ttl} and {max_ttl}, got {ttl} seconds")]
    InvalidTtl {
        max_ttl: String,
        min_ttl: String,
        ttl: u64,
    },
}

pub struct HttpCacheBuilder {
    db_path: Option<PathBuf>,
    default_ttl: Option<Duration>,
    max_size: Option<ByteSize>,
}

impl HttpCacheBuilder {
    // `db_path`: location of sqlite file for file-based db. If `None`, the db will be created in-memory.
    pub fn new(db_path: Option<impl Into<PathBuf>>) -> Self {
        Self {
            db_path: db_path.map(|x| x.into()),
            default_ttl: None,
            max_size: None,
        }
    }

    /// Attempt to build the HttpCache, with multiple fallbacks.
    /// 1. If a `db_path` is provided, the parameters are validated, and the SQLite connection is successful, we use a FS-based SQLite database.
    /// 2. If no `db_path` is provided, or the SQLite connection fails, but `validate()` is successful, we use an in-memory SQLite database.
    /// 3. If both of those fail, we use an in-memory Rust structural store.
    ///
    /// Steps 1 and 2 are handled by a call to `build_valid()`. After any fallback step, we emit a build cache error and continue.
    pub fn build(&mut self) -> HttpCache {
        match self.build_valid() {
            Ok(cache) => cache,
            Err(e) => {
                telemetry::record_build_cache_error(&e);
                HttpCache {
                    default_ttl: DEFAULT_TTL,
                    max_size: DEFAULT_MAX_SIZE,
                    store: HttpCacheStore::new(Box::new(self.open_rust_db())),
                }
            }
        }
    }

    /// Attempt to fallibly build the HttpCache in SQLite, only returning an error if validation step fails.
    /// 1. If a `db_path` is provided, the parameters are validated, and the SQLite connection is successful, we use a FS-based SQLite database.
    /// 2. If no `db_path` is provided, or the SQLite connection fails, but `validate()` is successful, we use an in-memory SQLite database.
    ///
    /// If the validation step fails, an error is returned.
    /// After any fallback step, we emit a build cache error and continue.
    pub fn build_valid(&self) -> Result<HttpCache, HttpCacheBuilderError> {
        self.validate()?;

        let db = self.open_sqlite_db()?;
        let max_size = self.max_size.unwrap_or(DEFAULT_MAX_SIZE);
        let store = HttpCacheStore::new(Box::new(db));
        let default_ttl = self.default_ttl.unwrap_or(DEFAULT_TTL);

        Ok(HttpCache {
            default_ttl,
            max_size,
            store,
        })
    }

    #[cfg(test)]
    pub fn build_valid_for_time_dependent_tests(&self) -> Result<HttpCache, HttpCacheBuilderError> {
        self.validate()?;

        let db = self.open_sqlite_db()?;
        let max_size = self.max_size.unwrap_or(DEFAULT_MAX_SIZE);
        let store = HttpCacheStore::new_with_test_clock(Box::new(db));
        let default_ttl = self.default_ttl.unwrap_or(DEFAULT_TTL);

        Ok(HttpCache {
            default_ttl,
            max_size,
            store,
        })
    }

    pub fn default_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = Some(ttl);
        self
    }

    pub fn max_size(mut self, max_size: ByteSize) -> Self {
        self.max_size = Some(max_size);
        self
    }

    // Opens the db at `db_path`, or a memory db if there is no path or it fails to open.
    fn open_sqlite_db(
        &self,
    ) -> Result<LazyDb<HttpCacheConnectionInitializer>, HttpCacheBuilderError> {
        if let Some(db_path) = &self.db_path {
            match open_connection(db_path) {
                Ok(db) => return Ok(db),
                Err(e) => telemetry::record_build_cache_error(&HttpCacheBuilderError::from(e)),
            }
        }

        // TODO: This is cast to DatabaseMemory rather than Database. Remove this when DatabaseMemory is removed.
        open_connection(Path::new(IN_MEMORY_DB_PATH)).map_err(HttpCacheBuilderError::DatabaseMemory)
    }

    // Creates a fully structural in-Rust memory 'database'.
    // Non-durable alternative to open_sqlite_connection
    // TODO: This is intentionally redundant with the sqlite in-memory one. Eventually, one or the other will be removed.
    fn open_rust_db(&mut self) -> HttpCacheMemory {
        HttpCacheMemory::new()
    }

    fn validate(&self) -> Result<(), HttpCacheBuilderError> {
        if self
            .db_path
            .as_ref()
            .map(|p| p.to_string_lossy().trim().is_empty())
            .unwrap_or(false)
        {
            return Err(HttpCacheBuilderError::EmptyDbPath);
        }

        if let Some(max_size) = self.max_size {
            if max_size < MIN_CACHE_SIZE || max_size > MAX_CACHE_SIZE {
                return Err(HttpCacheBuilderError::InvalidMaxSize {
                    max_size: MAX_CACHE_SIZE.to_string(),
                    min_size: MIN_CACHE_SIZE.to_string(),
                    size_bytes: max_size.as_u64(),
                });
            }
        }

        if let Some(ttl) = self.default_ttl {
            if !(MIN_TTL..=MAX_TTL).contains(&ttl) {
                return Err(HttpCacheBuilderError::InvalidTtl {
                    max_ttl: format!("{} seconds", MAX_TTL.as_secs()),
                    min_ttl: format!("{} seconds", MIN_TTL.as_secs()),
                    ttl: ttl.as_secs(),
                });
            }
        }

        Ok(())
    }
}

// Opens the db at `path`, eagerly opening the initial connection so failures surface here.
fn open_connection(
    path: &Path,
) -> Result<LazyDb<HttpCacheConnectionInitializer>, open_database::Error> {
    let db = LazyDb::new(
        path,
        OpenFlags::default(),
        HttpCacheConnectionInitializer {},
    );
    let _ = db.lock()?;
    Ok(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_builder(path: &str) -> HttpCacheBuilder {
        HttpCacheBuilder::new(Some(path))
    }

    #[test]
    fn test_cache_builder_with_defaults() {
        let builder = make_test_builder(IN_MEMORY_DB_PATH);
        assert_eq!(builder.db_path, Some(PathBuf::from(IN_MEMORY_DB_PATH)));
        assert_eq!(builder.max_size, None);
        assert_eq!(builder.default_ttl, None);
        assert!(builder.build_valid().is_ok());
    }

    #[test]
    fn test_unopenable_path_falls_back_to_memory_db() {
        // The fallback records telemetry, so don't race the telemetry tests.
        let _lock = crate::telemetry::test_lock();
        let cache = make_test_builder("this/is/a/test/bad/path")
            .build_valid()
            .unwrap();
        assert!(cache.is_memory().unwrap());
    }

    #[test]
    fn test_cache_builder_valid_custom() {
        let builder = make_test_builder(IN_MEMORY_DB_PATH)
            .max_size(ByteSize::b(1024))
            .default_ttl(Duration::from_secs(60));

        assert_eq!(builder.db_path, Some(PathBuf::from(IN_MEMORY_DB_PATH)));
        assert_eq!(builder.max_size, Some(ByteSize::b(1024)));
        assert_eq!(builder.default_ttl, Some(Duration::from_secs(60)));
        assert!(builder.build_valid().is_ok());
    }

    #[test]
    fn test_validation_empty_db_path() {
        let result = make_test_builder("   ").build_valid();
        assert!(matches!(result, Err(HttpCacheBuilderError::EmptyDbPath)));
    }

    #[test]
    fn test_validation_max_size_too_small() {
        let result = make_test_builder(IN_MEMORY_DB_PATH)
            .max_size(ByteSize::b(512))
            .build_valid();
        assert!(matches!(
            result,
            Err(HttpCacheBuilderError::InvalidMaxSize {
                size_bytes: 512,
                min_size: _,
                max_size: _,
            })
        ));
    }

    #[test]
    fn test_validation_max_size_too_large() {
        let result = make_test_builder(IN_MEMORY_DB_PATH)
            .max_size(ByteSize::b(2 * 1024 * 1024 * 1024))
            .build_valid();
        assert!(matches!(
            result,
            Err(HttpCacheBuilderError::InvalidMaxSize {
                size_bytes: 2147483648,
                min_size: _,
                max_size: _,
            })
        ));
    }

    #[test]
    fn test_validation_max_size_boundaries() {
        let builder_min = make_test_builder(IN_MEMORY_DB_PATH).max_size(MIN_CACHE_SIZE);
        assert!(builder_min.build_valid().is_ok());

        let builder_max = make_test_builder(IN_MEMORY_DB_PATH).max_size(MAX_CACHE_SIZE);
        assert!(builder_max.build_valid().is_ok());
    }

    #[test]
    fn test_validation_ttl_too_small() {
        let result = make_test_builder(IN_MEMORY_DB_PATH)
            .default_ttl(Duration::from_secs(0))
            .build_valid();
        assert!(matches!(
            result,
            Err(HttpCacheBuilderError::InvalidTtl {
                ttl: 0,
                min_ttl: _,
                max_ttl: _,
            })
        ));
    }

    #[test]
    fn test_validation_ttl_too_large() {
        let result = make_test_builder(IN_MEMORY_DB_PATH)
            .default_ttl(Duration::from_secs(8 * 24 * 60 * 60))
            .build_valid();
        assert!(matches!(
            result,
            Err(HttpCacheBuilderError::InvalidTtl {
                ttl: 691200,
                min_ttl: _,
                max_ttl: _,
            })
        ));
    }

    #[test]
    fn test_validation_ttl_boundaries() {
        let builder_min = make_test_builder(IN_MEMORY_DB_PATH).default_ttl(MIN_TTL);
        assert!(builder_min.build_valid().is_ok());

        let builder_max = make_test_builder(IN_MEMORY_DB_PATH).default_ttl(MAX_TTL);
        assert!(builder_max.build_valid().is_ok());
    }

    #[test]
    fn example_error_still_builds_in_memory() {
        // Normally built memory database with reasonable bounds should be built in SQLite memory.
        let result = make_test_builder(":memory:").build();
        assert!(result.is_memory().unwrap());
        assert!(result.is_sqlite());

        // When built with unreasonable invalidated bounds, it should not be in SQLite.
        let result = make_test_builder("test.db")
            .default_ttl(Duration::from_secs(8 * 24 * 60 * 60))
            .build();
        assert!(result.is_memory().unwrap());
        assert!(!result.is_sqlite());
    }
}

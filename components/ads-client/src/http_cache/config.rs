/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::bytesize::ByteSize;
use std::time::Duration;

const DEFAULT_MAX_SIZE: ByteSize = ByteSize::mib(10);
const DEFAULT_TTL: Duration = Duration::from_secs(300);

const MIN_CACHE_SIZE_BYTES: ByteSize = ByteSize::kib(1);
const MAX_CACHE_SIZE_BYTES: ByteSize = ByteSize::mib(100);
const MIN_TTL_SECONDS: u64 = 1;
const MAX_TTL_SECONDS: u64 = 24 * 60 * 60;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum HttpCacheConfigError {
    #[error("Maximum cache size must be between 1 KB and 100 MB, got {size_bytes} bytes")]
    InvalidMaxSize { size_bytes: u64 },

    #[error("TTL must be between 1 second and 24 hours, got {ttl_seconds} seconds")]
    InvalidTtl { ttl_seconds: u64 },

    #[error("Database path cannot be empty")]
    EmptyDbPath,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HttpCacheConfig {
    pub db_path: String,
    pub max_size_bytes: Option<u64>,
    pub ttl_seconds: Option<u64>,
}

impl HttpCacheConfig {
    pub fn validate(&self) -> Result<(), HttpCacheConfigError> {
        if self.db_path.trim().is_empty() {
            return Err(HttpCacheConfigError::EmptyDbPath);
        }

        if let Some(max_size_bytes) = self.max_size_bytes {
            if max_size_bytes < MIN_CACHE_SIZE_BYTES.as_u64()
                || max_size_bytes > MAX_CACHE_SIZE_BYTES.as_u64()
            {
                return Err(HttpCacheConfigError::InvalidMaxSize {
                    size_bytes: max_size_bytes,
                });
            }
        }

        if let Some(ttl_seconds) = self.ttl_seconds {
            if !(MIN_TTL_SECONDS..=MAX_TTL_SECONDS).contains(&ttl_seconds) {
                return Err(HttpCacheConfigError::InvalidTtl { ttl_seconds });
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct HttpCacheConfigInner {
    pub db_path: String,
    pub max_size: ByteSize,
    pub ttl: Duration,
}

impl TryFrom<HttpCacheConfig> for HttpCacheConfigInner {
    type Error = HttpCacheConfigError;

    fn try_from(config: HttpCacheConfig) -> Result<Self, Self::Error> {
        config.validate()?;

        Ok(Self {
            db_path: config.db_path,
            max_size: config
                .max_size_bytes
                .map(ByteSize::b)
                .unwrap_or(DEFAULT_MAX_SIZE),
            ttl: config
                .ttl_seconds
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_TTL),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config_with_defaults() {
        let config = HttpCacheConfig {
            db_path: "test.db".to_string(),
            max_size_bytes: None,
            ttl_seconds: None,
        };

        let internal_config = HttpCacheConfigInner::try_from(config).unwrap();
        assert_eq!(internal_config.db_path, "test.db");
        assert_eq!(internal_config.max_size, DEFAULT_MAX_SIZE);
        assert_eq!(internal_config.ttl, DEFAULT_TTL);
    }

    #[test]
    fn test_cache_config_valid_custom() {
        let custom_config = HttpCacheConfig {
            db_path: "custom.db".to_string(),
            max_size_bytes: Some(1024),
            ttl_seconds: Some(60),
        };

        let internal_custom = HttpCacheConfigInner::try_from(custom_config).unwrap();
        assert_eq!(internal_custom.db_path, "custom.db");
        assert_eq!(internal_custom.max_size.as_u64(), 1024);
        assert_eq!(internal_custom.ttl.as_secs(), 60);
    }

    #[test]
    fn test_validation_empty_db_path() {
        let config = HttpCacheConfig {
            db_path: "   ".to_string(),
            max_size_bytes: None,
            ttl_seconds: None,
        };

        let result = config.validate();
        assert!(matches!(result, Err(HttpCacheConfigError::EmptyDbPath)));
    }

    #[test]
    fn test_validation_max_size_too_small() {
        let config = HttpCacheConfig {
            db_path: "test.db".to_string(),
            max_size_bytes: Some(512),
            ttl_seconds: None,
        };

        let result = config.validate();
        assert!(matches!(
            result,
            Err(HttpCacheConfigError::InvalidMaxSize { size_bytes: 512 })
        ));
    }

    #[test]
    fn test_validation_max_size_too_large() {
        let config = HttpCacheConfig {
            db_path: "test.db".to_string(),
            max_size_bytes: Some(2 * 1024 * 1024 * 1024),
            ttl_seconds: None,
        };

        let result = config.validate();
        assert!(matches!(
            result,
            Err(HttpCacheConfigError::InvalidMaxSize {
                size_bytes: 2147483648
            })
        ));
    }

    #[test]
    fn test_validation_max_size_boundaries() {
        let config_min = HttpCacheConfig {
            db_path: "test.db".to_string(),
            max_size_bytes: Some(MIN_CACHE_SIZE_BYTES.as_u64()),
            ttl_seconds: None,
        };
        assert!(config_min.validate().is_ok());

        let config_max = HttpCacheConfig {
            db_path: "test.db".to_string(),
            max_size_bytes: Some(MAX_CACHE_SIZE_BYTES.as_u64()),
            ttl_seconds: None,
        };
        assert!(config_max.validate().is_ok());
    }

    #[test]
    fn test_validation_ttl_too_small() {
        let config = HttpCacheConfig {
            db_path: "test.db".to_string(),
            max_size_bytes: None,
            ttl_seconds: Some(0),
        };

        let result = config.validate();
        assert!(matches!(
            result,
            Err(HttpCacheConfigError::InvalidTtl { ttl_seconds: 0 })
        ));
    }

    #[test]
    fn test_validation_ttl_too_large() {
        let config = HttpCacheConfig {
            db_path: "test.db".to_string(),
            max_size_bytes: None,
            ttl_seconds: Some(25 * 60 * 60),
        };

        let result = config.validate();
        assert!(matches!(
            result,
            Err(HttpCacheConfigError::InvalidTtl { ttl_seconds: 90000 })
        ));
    }

    #[test]
    fn test_validation_ttl_boundaries() {
        let config_min = HttpCacheConfig {
            db_path: "test.db".to_string(),
            max_size_bytes: None,
            ttl_seconds: Some(MIN_TTL_SECONDS),
        };
        assert!(config_min.validate().is_ok());

        let config_max = HttpCacheConfig {
            db_path: "test.db".to_string(),
            max_size_bytes: None,
            ttl_seconds: Some(MAX_TTL_SECONDS),
        };
        assert!(config_max.validate().is_ok());
    }

    #[test]
    fn test_validation_multiple_errors() {
        let config = HttpCacheConfig {
            db_path: "".to_string(),
            max_size_bytes: Some(512),
            ttl_seconds: Some(0),
        };

        let result = config.validate();
        assert!(matches!(result, Err(HttpCacheConfigError::EmptyDbPath)));
    }

    #[test]
    fn test_try_from_with_validation_error() {
        let config = HttpCacheConfig {
            db_path: "".to_string(),
            max_size_bytes: Some(1024),
            ttl_seconds: Some(60),
        };

        let result = HttpCacheConfigInner::try_from(config);
        assert!(matches!(result, Err(HttpCacheConfigError::EmptyDbPath)));
    }
}

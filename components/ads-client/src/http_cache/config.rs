use std::time::Duration;

const DEFAULT_DB_PATH: &str = "http_cache.sqlite";
const DEFAULT_MAX_SIZE: ByteSize = ByteSize::mib(10);
const DEFAULT_TTL: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ByteSize(u64);

impl ByteSize {
    pub const fn b(value: u64) -> Self {
        Self(value)
    }

    pub const fn mib(value: u64) -> Self {
        Self(value * 1024 * 1024)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct HttpCacheConfig {
    pub db_path: Option<String>,
    pub max_size_bytes: Option<u64>,
    pub ttl_seconds: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct HttpCacheConfigInner {
    pub db_path: String,
    pub max_size: ByteSize,
    pub ttl: Duration,
}

impl From<HttpCacheConfig> for HttpCacheConfigInner {
    fn from(config: HttpCacheConfig) -> Self {
        Self {
            db_path: config
                .db_path
                .unwrap_or_else(|| DEFAULT_DB_PATH.to_string()),
            max_size: config
                .max_size_bytes
                .map(ByteSize::b)
                .unwrap_or(DEFAULT_MAX_SIZE),
            ttl: config
                .ttl_seconds
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_TTL),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_config() {
        let default_config = HttpCacheConfig::default();

        let internal_default = HttpCacheConfigInner::from(default_config);
        assert_eq!(internal_default.db_path, DEFAULT_DB_PATH);
        assert_eq!(internal_default.max_size, DEFAULT_MAX_SIZE);
        assert_eq!(internal_default.ttl, DEFAULT_TTL);

        let custom_config = HttpCacheConfig {
            db_path: Some("custom.db".to_string()),
            max_size_bytes: Some(1024),
            ttl_seconds: Some(60),
        };

        let internal_custom = HttpCacheConfigInner::from(custom_config);
        assert_eq!(internal_custom.db_path, "custom.db");
        assert_eq!(internal_custom.max_size.as_u64(), 1024);
        assert_eq!(internal_custom.ttl.as_secs(), 60);
    }
}

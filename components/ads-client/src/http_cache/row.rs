use rusqlite::Row;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use viaduct::{header_names, Header, Request, Response};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CacheControlDirectives {
    pub max_age: Option<u64>,
    pub must_revalidate: bool,
    pub no_cache: bool,
    pub no_store: bool,
    pub private: bool,
}

impl CacheControlDirectives {
    pub fn from_header(cache_control: Option<&str>) -> Self {
        let mut directives = Self {
            max_age: None,
            must_revalidate: false,
            no_cache: false,
            no_store: false,
            private: false,
        };

        if let Some(header_value) = cache_control {
            for directive in header_value.split(',').map(|s| s.trim()) {
                match directive {
                    "no-cache" => directives.no_cache = true,
                    "no-store" => directives.no_store = true,
                    "must-revalidate" => directives.must_revalidate = true,
                    "private" => directives.private = true,
                    s if s.starts_with("max-age=") => {
                        if let Some(age_str) = s.strip_prefix("max-age=") {
                            directives.max_age = age_str.parse().ok();
                        }
                    }
                    _ => {}
                }
            }
        }

        directives
    }

    pub fn should_cache(&self) -> bool {
        !self.no_store
    }

    pub fn get_ttl(&self, default_ttl: Duration) -> Duration {
        self.max_age.map(Duration::from_secs).unwrap_or(default_ttl)
    }
}

#[derive(Clone, Debug)]
pub struct HttpCacheRow {
    pub cache_control: CacheControlDirectives,
    pub cached_at: i64,
    pub etag: Option<String>,
    pub request_hash: String,
    pub response_body: Vec<u8>,
    pub response_headers: Vec<u8>,
    pub response_status: i64,
    pub size: i64,
}

impl HttpCacheRow {
    pub fn from_request_response(request: &Request, response: &Response) -> Self {
        let headers_map: HashMap<String, String> = response.headers.clone().into();
        let response_headers = serde_json::to_vec(&headers_map).unwrap_or_default();
        let etag = response
            .headers
            .get(header_names::ETAG)
            .map(|s| s.trim_matches('"').to_string());
        let cache_control =
            CacheControlDirectives::from_header(response.headers.get(header_names::CACHE_CONTROL));
        let size = (response_headers.len()
            + response.body.len()
            + request.body.as_ref().map_or(0, |b| b.len())) as i64;

        Self {
            cache_control,
            cached_at: Self::now_epoch(),
            etag,
            request_hash: Self::hash_request(request),
            response_body: response.body.clone(),
            response_headers,
            response_status: response.status as i64,
            size,
        }
    }

    pub fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        let cache_control_str: Option<String> = row.get(0)?;
        let cache_control = cache_control_str
            .and_then(|json_str| serde_json::from_str(&json_str).ok())
            .unwrap_or_else(|| CacheControlDirectives::from_header(None));

        Ok(Self {
            cache_control,
            cached_at: row.get(1)?,
            etag: row.get(2)?,
            request_hash: row.get(3)?,
            response_body: row.get(4)?,
            response_headers: row.get(5)?,
            response_status: row.get(6)?,
            size: row.get(7)?,
        })
    }

    pub fn to_response(&self, request: &Request) -> Response {
        let headers = serde_json::from_slice::<HashMap<String, String>>(&self.response_headers)
            .map(|map| {
                map.into_iter()
                    .filter_map(|(n, v)| Header::new(n, v).ok())
                    .collect::<Vec<_>>()
                    .into()
            })
            .unwrap_or_else(|_| viaduct::Headers::new());

        Response {
            request_method: request.method,
            url: request.url.clone(),
            status: self.response_status as u16,
            headers,
            body: self.response_body.clone(),
        }
    }

    pub fn hash_request(request: &Request) -> String {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        request.method.hash(&mut hasher);
        request.url.hash(&mut hasher);

        for header in request.headers.clone().into_iter() {
            header.name.hash(&mut hasher);
            header.value.hash(&mut hasher);
        }

        request.body.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    pub fn now_epoch() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viaduct::{Headers, Method, Request};

    fn create_test_request(url: &str, body: &[u8]) -> Request {
        Request {
            method: Method::Get,
            url: url.parse().unwrap(),
            headers: Headers::new(),
            body: Some(body.to_vec()),
        }
    }

    #[test]
    fn test_request_hashing() {
        let request1 = create_test_request("https://example.com/api1", b"body1");
        let request2 = create_test_request("https://example.com/api2", b"body2");
        let request3 = create_test_request("https://example.com/api", b"body");

        let hash1 = HttpCacheRow::hash_request(&request1);
        let hash2 = HttpCacheRow::hash_request(&request2);
        let hash3 = HttpCacheRow::hash_request(&request3);

        assert_ne!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_ne!(hash2, hash3);

        let same_request = create_test_request("https://example.com/api", b"body");
        let hash4 = HttpCacheRow::hash_request(&same_request);
        assert_eq!(hash3, hash4);
    }

    #[test]
    fn test_cache_control_parsing() {
        // Test max-age
        let directives = CacheControlDirectives::from_header(Some("max-age=3600"));
        assert_eq!(directives.max_age, Some(3600));
        assert!(!directives.no_cache);
        assert!(!directives.no_store);

        // Test no-cache and no-store
        let directives = CacheControlDirectives::from_header(Some("no-cache, no-store"));
        assert!(directives.no_cache);
        assert!(directives.no_store);
        assert!(!directives.should_cache());

        // Test multiple directives
        let directives =
            CacheControlDirectives::from_header(Some("max-age=1800, must-revalidate, private"));
        assert_eq!(directives.max_age, Some(1800));
        assert!(directives.must_revalidate);
        assert!(directives.private);

        // Test empty header
        let directives = CacheControlDirectives::from_header(None);
        assert_eq!(directives.max_age, None);
        assert!(!directives.no_cache);
        assert!(!directives.no_store);
        assert!(directives.should_cache());
    }

    #[test]
    fn test_cache_control_ttl() {
        let default_ttl = Duration::from_secs(300);

        // Test with max-age
        let directives = CacheControlDirectives::from_header(Some("max-age=3600"));
        assert_eq!(directives.get_ttl(default_ttl), Duration::from_secs(3600));

        // Test without max-age
        let directives = CacheControlDirectives::from_header(Some("no-cache"));
        assert_eq!(directives.get_ttl(default_ttl), default_ttl);
    }
}

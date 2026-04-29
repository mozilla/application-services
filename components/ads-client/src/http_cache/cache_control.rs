/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde::{Deserialize, Serialize};
use std::time::Duration;
use viaduct::{header_names, Response};

use super::MAX_TTL;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CacheControl {
    pub max_age: Option<u64>,
    pub must_revalidate: bool,
    pub no_cache: bool,
    pub no_store: bool,
    pub private: bool,
}

impl From<&Response> for CacheControl {
    fn from(response: &Response) -> Self {
        let mut cache_control = Self {
            max_age: None,
            must_revalidate: false,
            no_cache: false,
            no_store: false,
            private: false,
        };

        if let Some(header) = response.headers.get(header_names::CACHE_CONTROL) {
            for directive in header.split(',').map(|s| s.trim()) {
                match directive {
                    "must-revalidate" => cache_control.must_revalidate = true,
                    "no-cache" => cache_control.no_cache = true,
                    "no-store" => cache_control.no_store = true,
                    "private" => cache_control.private = true,
                    s if s.starts_with("max-age=") => {
                        if let Some(age_str) = s.strip_prefix("max-age=") {
                            cache_control.max_age = age_str.parse().ok();
                        }
                    }
                    _ => {}
                }
            }
        }

        cache_control
    }
}

impl CacheControl {
    pub fn should_cache(&self) -> bool {
        !self.no_store
    }

    /// Resolve the TTL to use when storing a response in the cache.
    ///
    /// Priority (highest to lowest):
    /// 1. `explicit_ttl` — caller-provided per-request override.
    /// 2. Server `Cache-Control: max-age` from this response.
    /// 3. `default_ttl` — the cache's configured default.
    ///
    /// The resulting TTL is capped at [`MAX_TTL`] for safety.
    pub fn effective_ttl(
        &self,
        explicit_ttl: Option<Duration>,
        default_ttl: Duration,
    ) -> Duration {
        let chosen = explicit_ttl
            .or_else(|| self.max_age.map(Duration::from_secs))
            .unwrap_or(default_ttl);
        std::cmp::min(chosen, MAX_TTL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viaduct::{Headers, Method};

    fn from_header(header: Option<&str>) -> CacheControl {
        let mut headers = Headers::new();
        if let Some(header) = header {
            headers.insert(header_names::CACHE_CONTROL, header).unwrap();
        }
        CacheControl::from(&Response {
            body: b"".to_vec(),
            headers,
            request_method: Method::Get,
            status: 200,
            url: "https://example.com".parse().unwrap(),
        })
    }

    #[test]
    fn test_cache_control_parsing() {
        // Test max-age
        let directives = from_header(Some("max-age=3600"));
        assert_eq!(directives.max_age, Some(3600));
        assert!(!directives.no_cache);
        assert!(!directives.no_store);

        // Test no-cache and no-store
        let directives = from_header(Some("no-cache, no-store"));
        assert!(directives.no_cache);
        assert!(directives.no_store);

        // Test multiple directives
        let directives = from_header(Some("max-age=1800, must-revalidate, private"));
        assert_eq!(directives.max_age, Some(1800));
        assert!(directives.must_revalidate);
        assert!(directives.private);

        // Test empty header
        let directives = from_header(None);
        assert_eq!(directives.max_age, None);
        assert!(!directives.no_cache);
        assert!(!directives.no_store);
        assert!(directives.should_cache());
    }

    #[test]
    fn effective_ttl_explicit_overrides_max_age_and_default() {
        let directives = from_header(Some("max-age=3600"));
        let ttl = directives.effective_ttl(
            Some(Duration::from_secs(60)),
            Duration::from_secs(300),
        );
        assert_eq!(ttl, Duration::from_secs(60));
    }

    #[test]
    fn effective_ttl_falls_back_to_max_age_when_no_explicit() {
        let directives = from_header(Some("max-age=3600"));
        let ttl = directives.effective_ttl(None, Duration::from_secs(300));
        assert_eq!(ttl, Duration::from_secs(3600));
    }

    #[test]
    fn effective_ttl_falls_back_to_default_when_no_max_age_and_no_explicit() {
        let directives = from_header(None);
        let ttl = directives.effective_ttl(None, Duration::from_secs(300));
        assert_eq!(ttl, Duration::from_secs(300));
    }

    #[test]
    fn effective_ttl_caps_max_age_at_max_ttl() {
        // 30 days, well over MAX_TTL (7 days)
        let directives = from_header(Some("max-age=2592000"));
        let ttl = directives.effective_ttl(None, Duration::from_secs(300));
        assert_eq!(ttl, MAX_TTL);
    }

    #[test]
    fn effective_ttl_caps_explicit_at_max_ttl() {
        let directives = from_header(None);
        let ttl = directives.effective_ttl(
            Some(Duration::from_secs(60 * 60 * 24 * 30)),
            Duration::from_secs(300),
        );
        assert_eq!(ttl, MAX_TTL);
    }

    #[test]
    fn effective_ttl_zero_max_age_yields_zero() {
        // max-age=0 should propagate as zero so the strategy emits NoCache.
        let directives = from_header(Some("max-age=0"));
        let ttl = directives.effective_ttl(None, Duration::from_secs(300));
        assert_eq!(ttl, Duration::ZERO);
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde::{Deserialize, Serialize};
use std::time::Duration;

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

    pub fn should_not_cache(&self) -> bool {
        self.no_store
    }

    pub fn should_cache(&self) -> bool {
        !self.should_not_cache()
    }

    pub fn get_ttl(&self, default_ttl: Duration) -> Duration {
        self.max_age.map(Duration::from_secs).unwrap_or(default_ttl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
        assert!(directives.should_not_cache());

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
        assert!(!directives.should_not_cache());
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

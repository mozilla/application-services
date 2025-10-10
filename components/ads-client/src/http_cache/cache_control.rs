/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde::{Deserialize, Serialize};
use viaduct::{header_names, Response};

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
        let header = response.headers.get(header_names::CACHE_CONTROL);

        let mut cache_control = Self {
            max_age: None,
            must_revalidate: false,
            no_cache: false,
            no_store: false,
            private: false,
        };

        if let Some(header_value) = header {
            for directive in header_value.split(',').map(|s| s.trim()) {
                match directive {
                    "no-cache" => cache_control.no_cache = true,
                    "no-store" => cache_control.no_store = true,
                    "must-revalidate" => cache_control.must_revalidate = true,
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
        !self.should_not_cache()
    }

    pub fn should_not_cache(&self) -> bool {
        self.no_store
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
        assert!(directives.should_not_cache());

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
        assert!(!directives.should_not_cache());
    }
}

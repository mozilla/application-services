/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::Row;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};
use viaduct::{header_names, Header, Request, Response};

use super::cache_control_directives::CacheControlDirectives;

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

        let mut headers: Vec<_> = request.headers.clone().into_iter().collect();
        headers.sort_by_key(|header| header.name.to_ascii_lowercase());
        for header in headers {
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
    fn test_request_hashing_header_order_and_case() {
        let base_url = "https://example.com/api";
        let body = b"body";

        let req_base = create_test_request(base_url, body);
        let mut h1 = Headers::new();
        h1.insert("Accept", "text/plain").unwrap();
        h1.insert("X-Test", "1").unwrap();
        let req1 = Request {
            headers: h1,
            ..req_base.clone()
        };

        let req_base = create_test_request(base_url, body);
        let mut h2 = Headers::new();
        h2.insert("X-Test", "1").unwrap();
        h2.insert("Accept", "text/plain").unwrap();
        let req2 = Request {
            headers: h2,
            ..req_base.clone()
        };

        let req_base = create_test_request(base_url, body);
        let mut h3 = Headers::new();
        h3.insert("accept", "text/plain").unwrap();
        h3.insert("x-test", "1").unwrap();
        let req3 = Request {
            headers: h3,
            ..req_base
        };

        let h_req1 = HttpCacheRow::hash_request(&req1);
        let h_req2 = HttpCacheRow::hash_request(&req2);
        let h_req3 = HttpCacheRow::hash_request(&req3);

        assert_eq!(h_req1, h_req2);
        assert_eq!(h_req1, h_req3);
    }
}

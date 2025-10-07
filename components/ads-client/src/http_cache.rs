mod config;
mod connection_initializer;
mod row;
mod store;

use self::{
    config::HttpCacheConfigInner, connection_initializer::HttpCacheConnectionInitializer,
    store::HttpCacheStore,
};

pub use config::HttpCacheConfig;
use sql_support::open_database;
use viaduct::{Request, Response, Result};

#[derive(uniffi::Object)]
pub struct HttpCache {
    store: HttpCacheStore,
}

#[uniffi::export]
impl HttpCache {
    #[uniffi::constructor]
    pub fn new(config: Option<HttpCacheConfig>) -> Self {
        let config = config.unwrap_or_default();
        let internal_config = HttpCacheConfigInner::from(config);
        let initializer = HttpCacheConnectionInitializer {};
        let conn = if cfg!(test) {
            open_database::open_memory_database(&initializer)
                .expect("failed to open memory cache db")
        } else {
            open_database::open_database(&internal_config.db_path, &initializer)
                .expect("failed to open cache db")
        };
        let store = HttpCacheStore::new(internal_config, conn);
        Self { store }
    }

    pub fn send(&self, request: Request) -> Result<Response> {
        self.store.cleanup_expired().ok();
        if let Some(resp) = self.store.lookup(&request).ok().flatten() {
            return Ok(resp);
        }
        let response = request.clone().send()?;
        if response.is_success() {
            self.store.store(&request, &response).ok();
            self.store.trim_to_max_size().ok();
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viaduct::{header_names, Headers, Method, Request, Response};

    fn create_test_request(url: &str, body: &[u8]) -> Request {
        Request {
            method: Method::Get,
            url: url.parse().unwrap(),
            headers: Headers::new(),
            body: Some(body.to_vec()),
        }
    }

    fn create_test_response(status: u16, body: &[u8]) -> Response {
        let mut headers = Headers::new();
        headers
            .insert(header_names::CONTENT_TYPE, "application/json")
            .unwrap();
        headers.insert(header_names::ETAG, "\"test-etag\"").unwrap();

        Response {
            request_method: Method::Get,
            url: "https://example.com/test".parse().unwrap(),
            status,
            headers,
            body: body.to_vec(),
        }
    }

    #[test]
    fn test_send_retrieves_cached_response() {
        let cache = HttpCache::new(None);

        let request = create_test_request("https://example.com/api", b"test body");
        let response = create_test_response(200, b"cached response");

        cache.store.store(&request, &response).unwrap();

        let result = cache.send(request);
        assert!(result.is_ok());
        let retrieved_response = result.unwrap();
        assert_eq!(retrieved_response.body, b"cached response");
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{collections::HashMap, sync::Arc, time::Duration};

use error_support::info;
use tokio::time::timeout;
use url::Url;

use crate::error::MapBackendError;
use crate::{init_backend, Backend, FairyBridgeError, Method, Request, Response, Result};

type Connector = hyper_tls::HttpsConnector<hyper::client::connect::HttpConnector>;
type Client = hyper::client::Client<Connector, hyper::Body>;

struct HyperBackend {
    runtime: tokio::runtime::Runtime,
    client: Client,
}

#[uniffi::export]
pub fn init_backend_hyper() -> Result<(), FairyBridgeError> {
    info!("initializing hyper backend");
    // Create a multi-threaded runtime, with 1 worker thread.
    //
    // This creates and manages a single worker thread.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let https_connector = hyper_tls::HttpsConnector::new();
    let client = hyper::Client::<()>::builder().build(https_connector);
    let backend = Arc::new(HyperBackend { runtime, client });
    init_backend(backend)
}

#[async_trait::async_trait]
impl Backend for HyperBackend {
    async fn send_request(self: Arc<Self>, request: Request) -> Result<Response, FairyBridgeError> {
        let handle = self.runtime.handle().clone();
        match handle
            .spawn(async move {
                let req_timeout = request.settings.timeout;
                let req = self.make_request_inner(request);
                if req_timeout == 0 {
                    req.await
                } else {
                    let duration = Duration::from_millis(req_timeout.into());
                    timeout(duration, req).await.unwrap_or_else(|_| {
                        Err(FairyBridgeError::new_backend_error("Request timeout"))
                    })
                }
            })
            .await
        {
            Ok(result) => result,
            Err(e) => Err(FairyBridgeError::BackendError {
                msg: format!("error spawning tokio task: {e}"),
            }),
        }
    }
}

impl HyperBackend {
    /// Inner portion of `make_request()`
    ///
    /// This expects to be run in a `tokio::spawn` closure
    async fn make_request_inner(&self, request: Request) -> Result<Response> {
        let mut url = request.url.clone();
        let mut resp = self.make_single_request(request.clone()).await?;
        let mut redirect_count = 0;
        while resp.status().is_redirection() {
            redirect_count += 1;
            if request.settings.redirect_limit != 0
                && redirect_count > request.settings.redirect_limit
            {
                return Err(FairyBridgeError::new_backend_error("Too many redirections"));
            }
            let Some(location) = resp.headers().get("location") else {
                return Err(FairyBridgeError::new_backend_error(
                    "location header missing",
                ));
            };
            url = Url::parse(location.to_str().map_backend_error()?)?;
            let new_request = Request {
                settings: request.settings.clone(),
                method: request.method,
                url: url.clone(),
                headers: request.headers.clone(),
                body: None,
            };
            resp = self.make_single_request(new_request).await?;
        }

        let status = resp.status().as_u16();

        let mut headers = HashMap::new();
        for (name, value) in resp.headers() {
            let name = name.as_str().to_string();
            let value = String::from_utf8_lossy(value.as_bytes()).to_string();
            headers.insert(name.as_str().to_string(), value);
        }
        Ok(Response {
            url,
            status,
            headers,
            body: resp.into_body(),
        })
    }

    async fn make_single_request(&self, request: Request) -> Result<hyper::Response<Vec<u8>>> {
        let mut builder = hyper::Request::builder()
            .uri(convert_url(request.url)?)
            .method(match request.method {
                Method::Get => hyper::Method::GET,
                Method::Head => hyper::Method::HEAD,
                Method::Post => hyper::Method::POST,
                Method::Put => hyper::Method::PUT,
                Method::Delete => hyper::Method::DELETE,
                Method::Connect => hyper::Method::CONNECT,
                Method::Options => hyper::Method::OPTIONS,
                Method::Trace => hyper::Method::TRACE,
                Method::Patch => hyper::Method::PATCH,
            });
        for (name, value) in request.headers {
            let name = hyper::http::HeaderName::from_bytes(name.as_bytes()).map_backend_error()?;
            let value = hyper::http::HeaderValue::from_str(&value).map_backend_error()?;
            builder.headers_mut().unwrap().insert(name, value);
        }
        let req = builder
            .body(hyper::Body::from(request.body.unwrap_or_default()))
            .map_backend_error()?;
        let (parts, body) = self
            .client
            .request(req)
            .await
            .map_backend_error()?
            .into_parts();
        let body = hyper::body::to_bytes(body).await.map_backend_error()?;
        Ok(hyper::Response::from_parts(parts, body.to_vec()))
    }
}

fn convert_url(url: Url) -> Result<hyper::Uri> {
    hyper::Uri::builder()
        .scheme(url.scheme())
        .authority(url.authority())
        .path_and_query(match url.query() {
            None => url.path().to_string(),
            Some(query) => format!("{}?{}", url.path(), query),
        })
        .build()
        .map_backend_error()
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Viaduct dev backend
//!
//! This implements a backend using `hyper`.
//! Unlike the `hyper` backend it does not support HTTPS.
//! This means it's clear to vendor into moz-central and won't bring in unwanted sub-dependencies, like `openssl`.
//! This is mainly intended for testing, where the HTTP-only restriction is not an issue.
use std::{sync::Arc, time::Duration};

use error_support::info;
use tokio::time::timeout;
use url::Url;

use viaduct::{
    error::MapBackendError, init_backend, Backend, ClientSettings, Header, Method, Request,
    Response, Result, ViaductError,
};

type Client = hyper::client::Client<hyper::client::connect::HttpConnector, hyper::Body>;

struct HyperBackend {
    runtime: tokio::runtime::Runtime,
    client: Client,
}

/// Initialize the `dev` backend.
///
/// This is intended to be used in tests.  It uses `hyper` without any HTTPS support.
pub fn init_backend_dev() {
    info!("initializing dev backend");
    // Create a multi-threaded runtime, with 1 worker thread.
    //
    // This creates and manages a single worker thread.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let client = hyper::Client::new();
    let backend = Arc::new(HyperBackend { runtime, client });
    // This is only used in the testing situations, so we can ignore any errors.  Errors happen
    // when multiple places try to initialize the backend, which is expected.  The first one will
    // take effect and all others can be safely ignored.
    let _ = init_backend(backend);
}

#[async_trait::async_trait]
impl Backend for HyperBackend {
    async fn send_request(&self, request: Request, settings: ClientSettings) -> Result<Response> {
        let handle = self.runtime.handle().clone();
        let client = self.client.clone();
        match handle
            .spawn(async move {
                let req_timeout = settings.timeout;
                let req = make_request_inner(client, request, settings);
                if req_timeout == 0 {
                    req.await
                } else {
                    let duration = Duration::from_millis(req_timeout.into());
                    timeout(duration, req)
                        .await
                        .unwrap_or_else(|_| Err(ViaductError::new_backend_error("Request timeout")))
                }
            })
            .await
        {
            Ok(result) => result,
            Err(e) => Err(ViaductError::new_backend_error(format!(
                "error spawning tokio task: {e}"
            ))),
        }
    }
}

/// Inner portion of `make_request()`
///
/// This expects to be run in a `tokio::spawn` closure
async fn make_request_inner(
    client: Client,
    request: Request,
    settings: ClientSettings,
) -> Result<Response> {
    let mut url = request.url.clone();
    let mut resp = make_single_request(&client, request.clone()).await?;
    let mut redirect_count = 0;
    while resp.status().is_redirection() {
        redirect_count += 1;
        if settings.redirect_limit != 0 && redirect_count > settings.redirect_limit {
            return Err(ViaductError::new_backend_error("Too many redirections"));
        }
        let Some(location) = resp.headers().get("location") else {
            return Err(ViaductError::new_backend_error("location header missing"));
        };
        url = Url::parse(location.to_str().map_backend_error()?)?;
        let new_request = Request {
            method: request.method,
            url: url.clone(),
            headers: request.headers.clone(),
            body: None,
        };
        resp = make_single_request(&client, new_request).await?;
    }

    let status = resp.status().as_u16();

    let mut headers = Vec::new();
    for (name, value) in resp.headers() {
        let name = name.as_str().to_string();
        let value = String::from_utf8_lossy(value.as_bytes()).to_string();
        headers.push(Header::new(name, value)?);
    }
    Ok(Response {
        request_method: request.method,
        url,
        status,
        headers: headers.into(),
        body: resp.into_body(),
    })
}

async fn make_single_request(
    client: &Client,
    request: Request,
) -> Result<hyper::Response<Vec<u8>>> {
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
    for h in request.headers.into_vec() {
        let name = hyper::http::HeaderName::from_bytes(h.name.as_bytes()).map_backend_error()?;
        let value = hyper::http::HeaderValue::from_str(&h.value).map_backend_error()?;
        builder.headers_mut().unwrap().insert(name, value);
    }
    let req = builder
        .body(hyper::Body::from(request.body.unwrap_or_default()))
        .map_backend_error()?;
    let (parts, body) = client.request(req).await.map_backend_error()?.into_parts();
    let body = hyper::body::to_bytes(body).await.map_backend_error()?;
    Ok(hyper::Response::from_parts(parts, body.to_vec()))
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

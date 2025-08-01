/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod errors;

use error_support::warn;
use errors::{backend_error, MapError};
use std::sync::Once;
use url::Url;
use viaduct::Backend;

pub type Result<T, E = viaduct::Error> = std::result::Result<T, E>;

type Connector = hyper_tls::HttpsConnector<hyper::client::connect::HttpConnector>;
type Client = hyper::client::Client<Connector, hyper::Body>;

fn make_request(
    runtime: &tokio::runtime::Runtime,
    client: &Client,
    request: viaduct::Request,
) -> Result<hyper::Response<Vec<u8>>, viaduct::Error> {
    let mut builder = hyper::Request::builder()
        .uri(url_into_hyper_uri(request.url)?)
        .method(match request.method {
            viaduct::Method::Get => hyper::Method::GET,
            viaduct::Method::Head => hyper::Method::HEAD,
            viaduct::Method::Post => hyper::Method::POST,
            viaduct::Method::Put => hyper::Method::PUT,
            viaduct::Method::Delete => hyper::Method::DELETE,
            viaduct::Method::Connect => hyper::Method::CONNECT,
            viaduct::Method::Options => hyper::Method::OPTIONS,
            viaduct::Method::Trace => hyper::Method::TRACE,
            viaduct::Method::Patch => hyper::Method::PATCH,
        });
    for h in request.headers {
        let name =
            hyper::http::HeaderName::from_bytes(h.name().as_bytes()).map_to_viaduct_error()?;
        let value = hyper::http::HeaderValue::from_str(h.value()).map_to_viaduct_error()?;
        builder.headers_mut().unwrap().insert(name, value);
    }
    let req = builder
        .body(hyper::Body::from(request.body.unwrap_or_default()))
        .map_to_viaduct_error()?;
    runtime
        .block_on(async move {
            let (parts, body) = client.request(req).await?.into_parts();
            let body = hyper::body::to_bytes(body).await?;
            hyper::Result::Ok(hyper::Response::from_parts(parts, body.to_vec()))
        })
        .map_to_viaduct_error()
}

pub struct DevBackend {
    runtime: tokio::runtime::Runtime,
    client: Client,
}

impl DevBackend {
    fn new() -> Self {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .enable_io()
            .build()
            .unwrap();
        let client = hyper::Client::<()>::builder().build(hyper_tls::HttpsConnector::new());
        Self { runtime, client }
    }
}

impl Backend for DevBackend {
    fn send(&self, request: viaduct::Request) -> Result<viaduct::Response, viaduct::Error> {
        viaduct::note_backend("dev");
        let request_method = request.method;
        let mut url = request.url.clone();
        let mut resp = make_request(&self.runtime, &self.client, request.clone())?;
        let mut redirect_count = 0;
        while resp.status().is_redirection() {
            redirect_count += 1;
            if redirect_count > 10 {
                return backend_error("Too many redirections");
            }
            let Some(location) = resp.headers().get("location") else {
                return backend_error("location header missing");
            };
            url = Url::parse(location.to_str().map_to_viaduct_error()?)?;
            let new_request = viaduct::Request {
                method: request.method,
                url: url.clone(),
                headers: request.headers.clone(),
                body: None,
            };
            resp = make_request(&self.runtime, &self.client, new_request)?;
        }

        let status = resp.status().as_u16();

        let mut headers = viaduct::Headers::with_capacity(resp.headers().len());
        for (k, v) in resp.headers() {
            let val = String::from_utf8_lossy(v.as_bytes()).to_string();
            let hname = match viaduct::HeaderName::new(k.as_str().to_owned()) {
                Ok(name) => name,
                Err(e) => {
                    // Ignore headers with invalid names, since nobody can look for them anyway.
                    warn!("Server sent back invalid header name: '{}'", e);
                    continue;
                }
            };
            // Not using Header::new since the error it returns is for request headers.
            headers.insert_header(viaduct::Header::new_unchecked(hname, val));
        }
        Ok(viaduct::Response {
            request_method,
            url,
            status,
            headers,
            body: resp.into_body(),
        })
    }
}

fn url_into_hyper_uri(url: url::Url) -> Result<hyper::Uri> {
    hyper::Uri::builder()
        .scheme(url.scheme())
        .authority(url.authority())
        .path_and_query(match url.query() {
            None => url.path().to_string(),
            Some(query) => format!("{}?{}", url.path(), query),
        })
        .build()
        .map_to_viaduct_error()
}

static INIT_DEV_BACKEND: Once = Once::new();

pub fn use_dev_backend() {
    INIT_DEV_BACKEND.call_once(|| {
        viaduct::set_backend(Box::leak(Box::new(DevBackend::new())))
            .expect("Backend already set (FFI)");
    })
}

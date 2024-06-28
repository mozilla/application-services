/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{init_backend, Backend, BackendSettings, FairyBridgeError, Method, Request, Response};
use std::{sync::Arc, time::Duration};

struct ReqwestBackend {
    runtime: tokio::runtime::Runtime,
    client: reqwest::Client,
}

#[uniffi::export]
pub fn init_backend_reqwest(settings: BackendSettings) -> Result<(), FairyBridgeError> {
    // Create a multi-threaded runtime, with 1 worker thread.
    //
    // This creates and manages a single worker thread.
    //
    // Tokio also provides the current thread runtime, which is a "single-threaded future executor".
    // However that means it needs to block a thread to run tasks.
    // I.e. `send_request` would block to while executing the request.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let mut client_builder = reqwest::Client::builder();
    if settings.connect_timeout > 0 {
        client_builder =
            client_builder.connect_timeout(Duration::from_millis(settings.connect_timeout as u64))
    }
    if settings.timeout > 0 {
        client_builder = client_builder.timeout(Duration::from_millis(settings.timeout as u64))
    }
    client_builder = client_builder.redirect(reqwest::redirect::Policy::limited(
        settings.redirect_limit as usize,
    ));
    let client = client_builder.build()?;
    let backend = Arc::new(ReqwestBackend { runtime, client });
    init_backend(backend)
}

#[async_trait::async_trait]
impl Backend for ReqwestBackend {
    async fn send_request(self: Arc<Self>, request: Request) -> Result<Response, FairyBridgeError> {
        let handle = self.runtime.handle().clone();
        match handle
            .spawn(async move {
                self.convert_response(self.make_request(request).await?)
                    .await
            })
            .await
        {
            Ok(result) => result,
            Err(e) => Err(FairyBridgeError::BackendError {
                msg: format!("tokio error: {e}"),
            }),
        }
    }
}

impl ReqwestBackend {
    async fn make_request(&self, request: Request) -> Result<reqwest::Response, FairyBridgeError> {
        let method = match request.method {
            Method::Get => reqwest::Method::GET,
            Method::Head => reqwest::Method::HEAD,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Delete => reqwest::Method::DELETE,
            Method::Connect => reqwest::Method::CONNECT,
            Method::Options => reqwest::Method::OPTIONS,
            Method::Trace => reqwest::Method::TRACE,
            Method::Patch => reqwest::Method::PATCH,
        };
        let mut builder = self.client.request(method, request.url);
        for (key, value) in request.headers {
            builder = builder.header(key, value);
        }
        if let Some(body) = request.body {
            builder = builder.body(body)
        }
        Ok(builder.send().await?)
    }

    async fn convert_response(
        &self,
        response: reqwest::Response,
    ) -> Result<Response, FairyBridgeError> {
        let url = response.url().to_string();
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_owned(),
                    String::from_utf8_lossy(v.as_bytes()).to_string(),
                )
            })
            .collect();
        let body = response.bytes().await?.into();

        Ok(Response {
            url,
            status,
            headers,
            body,
        })
    }
}

impl From<reqwest::Error> for FairyBridgeError {
    fn from(error: reqwest::Error) -> Self {
        match error.status() {
            Some(status) => FairyBridgeError::HttpError {
                code: status.as_u16(),
            },
            None => FairyBridgeError::BackendError {
                msg: format!("reqwest error: {error}"),
            },
        }
    }
}

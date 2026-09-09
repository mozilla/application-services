/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Viaduct Rust backend
//!
//! This implements a backend using `reqwest`.
//! Unlike the `dev` backend it also supports HTTPS.
//!
//! This transitively depends on `hyper-tls`, which we don't want to vendor into moz-central at this time.
//! Because of that, only use this for crates that aren't members of the moz-central workspace like CLIs and the ios megazord.

use std::sync::Arc;

use error_support::info;
use viaduct::MapBackendError;

struct Backend {
    runtime: tokio::runtime::Runtime,
    client: reqwest::Client,
}

/// Set the viaduct backend to the `reqwest`-based one with HTTPS support.
///
/// Named `viaduct_init_backend_rust` since that reads better on iOS/Swift where there aren't any
/// namespaces.  Once we move to UniFII 0.31 we can use the renaming feature to do this instead.
#[uniffi::export]
pub fn viaduct_init_backend_rust() {
    info!("initializing Rust backend");
    // Create a multi-threaded runtime, with 1 worker thread.
    let backend = Backend {
        runtime: tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap(),
        client: reqwest::Client::new(),
    };
    viaduct::init_backend(Arc::new(backend));
}

#[async_trait::async_trait]
impl viaduct::Backend for Backend {
    async fn send_request(
        &self,
        request: viaduct::Request,
        settings: viaduct::ClientSettings,
    ) -> viaduct::Result<viaduct::Response> {
        let client = self.client.clone();
        let join_handle = self
            .runtime
            .spawn(async move { send_request(&client, request, &settings).await });
        match join_handle.await {
            Ok(result) => result,
            Err(e) => Err(viaduct::ViaductError::BackendError(format!(
                "Tokio join error: {e}"
            ))),
        }
    }
}

async fn send_request(
    client: &reqwest::Client,
    mut request: viaduct::Request,
    settings: &viaduct::ClientSettings,
) -> viaduct::Result<viaduct::Response> {
    let mut response = send_single_request(client, &request, settings).await?;
    // Handle redirection
    //
    // reqwest has support for handling redirection, but the policy is owned into the `reqwest::Client`.
    // Each request can have a different `viaduct::ClientSettings` value,
    // which would mean either building a new client per request or managing a pool of clients.
    // Neither option seems great, so we handle redirection ourselves.
    let mut redirect_count = 0;
    while response.status().is_redirection() {
        redirect_count += 1;
        if redirect_count > settings.redirect_limit {
            return Err(viaduct::ViaductError::new_backend_error(
                "Too many redirections",
            ));
        }
        let Some(location) = response.headers().get(reqwest::header::LOCATION) else {
            return Err(viaduct::ViaductError::new_backend_error(
                "location header missing",
            ));
        };
        request = viaduct::Request {
            url: request.url.join(location.to_str().map_backend_error()?)?,
            // 303 requires that the method to switch GET
            // All other redirect codes allow keeping the same method
            method: if response.status() == reqwest::StatusCode::SEE_OTHER {
                viaduct::Method::Get
            } else {
                request.method
            },
            // Same headers as the original request
            headers: request.headers,
            // The body is always empty for the redirect
            body: None,
        };

        response = send_single_request(client, &request, settings).await?;
    }
    map_response(&request, response).await
}

async fn send_single_request(
    client: &reqwest::Client,
    request: &viaduct::Request,
    settings: &viaduct::ClientSettings,
) -> viaduct::Result<reqwest::Response> {
    let method = match &request.method {
        viaduct::Method::Get => reqwest::Method::GET,
        viaduct::Method::Head => reqwest::Method::HEAD,
        viaduct::Method::Post => reqwest::Method::POST,
        viaduct::Method::Put => reqwest::Method::PUT,
        viaduct::Method::Delete => reqwest::Method::DELETE,
        viaduct::Method::Connect => reqwest::Method::CONNECT,
        viaduct::Method::Options => reqwest::Method::OPTIONS,
        viaduct::Method::Trace => reqwest::Method::TRACE,
        viaduct::Method::Patch => reqwest::Method::PATCH,
    };

    let mut headers = reqwest::header::HeaderMap::with_capacity(request.headers.len());
    for h in request.headers.iter() {
        headers.insert(
            reqwest::header::HeaderName::try_from(h.name.as_str()).map_backend_error()?,
            reqwest::header::HeaderValue::try_from(&h.value).map_backend_error()?,
        );
    }
    if !headers.contains_key(reqwest::header::USER_AGENT) {
        let user_agent = settings.user_agent.clone().or_else(|| {
            viaduct::settings::GLOBAL_SETTINGS
                .read()
                .default_user_agent
                .clone()
        });
        if let Some(ua) = user_agent {
            headers.insert(
                reqwest::header::USER_AGENT,
                ua.try_into().map_backend_error()?,
            );
        }
    }

    let mut req = client.request(method, request.url.clone()).headers(headers);

    if let Some(body) = request.body.clone() {
        req = req.body(body);
    }

    req.send().await.map_backend_error()
}

async fn map_response(
    request: &viaduct::Request,
    mut response: reqwest::Response,
) -> viaduct::Result<viaduct::Response> {
    let mut resp_headers = viaduct::Headers::with_capacity(response.headers().len());
    for (name, value) in response.headers_mut().drain() {
        let Some(name) = name else {
            // Skip values where the header name is None
            // reqwest uses this to handle multiple values for the same header, but viaduct
            // doesn't support that
            continue;
        };
        resp_headers.insert(
            viaduct::HeaderName::new(name.as_str().to_string()).map_backend_error()?,
            value.to_str().map_backend_error()?,
        )?;
    }

    Ok(viaduct::Response {
        request_method: request.method,
        url: response.url().clone(),
        status: response.status().as_u16(),
        headers: resp_headers,
        body: response.bytes().await.map_backend_error()?.to_vec(),
    })
}

uniffi::setup_scaffolding!();

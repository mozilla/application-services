/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Viaduct dev backend
//!
//! This implements a viaduct backend using `minreq`, with no feature flags enabled.
//! This lets us have an HTTP client without any sub-dependencies that we don't want to vendor into moz-central, like `openssl`.
//! It's mainly intended for testing, where the HTTP-only restriction is not an issue.
//! `minreq` has a sync API, we implement an async version using a separate thread and a `mpsc` channel.
use std::sync::{mpsc, Arc};

use error_support::{error, info};
use url::Url;

use viaduct::{
    error::MapBackendError, init_backend, Backend, ClientSettings, Headers, Method, Request,
    Response, Result, ViaductError,
};

struct DevBackend {
    /// Channel used to communicate with the thread for this backend.
    channel: mpsc::Sender<Event>,
}

/// Event sent to the mpsc channel
enum Event {
    /// Send a request using `minreq`, then return the response using a oneshot channel.
    SendRequest {
        request: Request,
        settings: ClientSettings,
        channel: oneshot::Sender<Result<Response>>,
    },
    Quit,
}

/// Worker thread that we manage to send requests.
///
/// This processes events from the mpsc channel
fn worker_thread(channel: mpsc::Receiver<Event>) {
    loop {
        match channel.recv() {
            Err(e) => {
                error!("Error reading from channel: {e}");
                return;
            }
            Ok(Event::SendRequest {
                request,
                settings,
                channel,
            }) => {
                let result = send_request(request, settings);
                channel
                    .send(result)
                    .expect("Error sending to oneshot channel");
            }
            Ok(Event::Quit) => {
                info!("Saw Quit event, exiting");
                break;
            }
        }
    }
}

/// Handle `Event::SendRequest`
fn send_request(request: Request, settings: ClientSettings) -> Result<Response> {
    let method = match request.method {
        Method::Get => minreq::Method::Get,
        Method::Head => minreq::Method::Head,
        Method::Post => minreq::Method::Post,
        Method::Put => minreq::Method::Put,
        Method::Delete => minreq::Method::Delete,
        Method::Connect => minreq::Method::Connect,
        Method::Options => minreq::Method::Options,
        Method::Trace => minreq::Method::Trace,
        Method::Patch => minreq::Method::Patch,
    };
    let req = minreq::Request::new(method, request.url.to_string())
        .with_headers(
            request
                .headers
                .iter()
                .map(|h| (h.name().as_str(), h.value())),
        )
        // Convert timeout from ms to seconds, rounding up.
        .with_timeout(settings.timeout.div_ceil(1000) as u64)
        .with_body(request.body.unwrap_or_default());
    let mut resp = req.send().map_backend_error()?;
    Ok(Response {
        request_method: request.method,
        url: Url::parse(&resp.url)?,
        // Use `take` to take all headers, but not partially deconstruct the `Response`.
        // This lets us use `into_bytes()` below.
        headers: Headers::try_from_hashmap(std::mem::take(&mut resp.headers))?,
        status: resp.status_code as u16,
        body: resp.into_bytes(),
    })
}

/// Initialize the `dev` backend.
///
/// This is intended to be used in tests.
pub fn init_backend_dev() {
    info!("initializing dev backend");
    let backend = Arc::new(DevBackend::new());
    // Register our backend with viaduct.  This is only used in the testing situations, so we can
    // ignore any `BackendAlreadyInitialized` errors. The first call will take effect and all
    // others can be safely ignored.
    let _ = init_backend(backend);
}

impl DevBackend {
    fn new() -> Self {
        // Create a MPSC channel, this is how we will send requests asynchronously to the worker
        // thread.
        let (tx, rx) = mpsc::channel();
        // Spawn a worker thread to process events in our channel.
        // When `Self` is dropped, we'll send the `Quit` event to stop the thread.
        std::thread::spawn(move || {
            worker_thread(rx);
        });
        Self { channel: tx }
    }
}

impl Drop for DevBackend {
    fn drop(&mut self) {
        if let Err(e) = self.channel.send(Event::Quit) {
            error!("Error sending quit event: {e}");
        }
    }
}

#[async_trait::async_trait]
impl Backend for DevBackend {
    async fn send_request(
        &self,
        request: Request,
        settings: ClientSettings,
    ) -> Result<Response, ViaductError> {
        // Create a oneshot channel.  This is how the worker thread will send the response back
        let (oneshot_tx, oneshot_rx) = oneshot::channel();
        // Send the request to the worker thread.
        self.channel
            .send(Event::SendRequest {
                request,
                settings,
                channel: oneshot_tx,
            })
            .map_backend_error()?;
        // Await the response from the worker thread.
        oneshot_rx.await.expect("Error awaiting oneshot channel")
    }
}

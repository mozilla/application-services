/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use url::Url;

use crate::{
    client::{
        config::WorkerConfig,
        error::{BackgroundWorkerError, ComponentError},
        AdsClientInner,
    },
    mars::{
        ad_request::{AdPlacementRequest, AdRequestRelevance},
        MARSRequestOptions, ReportReason,
    },
    request::RequestQueue,
};
use std::{collections::HashSet, sync::Arc, thread::JoinHandle, time::Duration};

pub const ADS_CLIENT_WORKER_THREAD_NAME: &str = "ads-client.worker";

// Thread sleep delay to wait for new requests to queue before resolving them.
// This must be a very short amount of time to ensure ads are requested near-immediately on startup.
pub const ADS_CLIENT_WORKER_SLEEP_LENGTH_DEFAULT: Duration = Duration::from_millis(100);

pub struct BackgroundWorker {
    // Worker thread is not needed to be accessed, but the handle needs to be held by the structure.
    // If None, no worker thread is present, and attempts to write to the request queue will be rejected
    // (because no consumer of the request queue will exist)
    thread_handle: Option<JoinHandle<()>>,
    request_queue: RequestQueue,
}

impl BackgroundWorker {
    // Create a new worker to handle fire-and-forget requests.
    // `worker_batch`: Batch interval, defaulting to `ADS_CLIENT_WORKER_SLEEP_LENGTH_DEFAULT ms`.
    pub fn new(inner_client: Arc<AdsClientInner>, worker_config: WorkerConfig) -> BackgroundWorker {
        let worker_batch = worker_config
            .worker_batch
            .unwrap_or(ADS_CLIENT_WORKER_SLEEP_LENGTH_DEFAULT);
        let request_queue = RequestQueue::new();
        let worker_request_queue = request_queue.clone();

        // Attempt to spawn a background worker thread to handle requests.
        // If it fails, all fields of `BackgroundWorker` will be populated with None, and an error will be logged.
        let Some(worker_thread) = std::thread::Builder::new()
            .name(ADS_CLIENT_WORKER_THREAD_NAME.to_owned())
            .spawn(move || worker(&inner_client, &worker_request_queue, worker_batch)).inspect_err(|err| {
                // TODO: Replace this log with telemetry.
                error_support::error!("Failed to create ads-client worker thread `{ADS_CLIENT_WORKER_THREAD_NAME}` with: {err}");
            }).ok()
        else {
            return BackgroundWorker { thread_handle: None, request_queue }
        };

        BackgroundWorker {
            thread_handle: Some(worker_thread),
            request_queue,
        }
    }

    pub fn new_empty() -> BackgroundWorker {
        BackgroundWorker {
            thread_handle: None,
            request_queue: RequestQueue::new(),
        }
    }

    // Add to the background worker's queue a request. It may be queued or batched, depending on the request.
    pub fn dispatch(&self, request: DispatchRequest) -> Result<(), BackgroundWorkerError> {
        if self.thread_handle.is_some() {
            self.request_queue.push_request(request);
            Ok(())
        } else {
            Err(BackgroundWorkerError::Closed)
        }
    }
}

// Endless worker for background thread that synchronously run tasks in the order provided by the RequestQueue.
fn worker(
    inner_client: &Arc<AdsClientInner>,
    request_queue: &RequestQueue,
    batch_duration: Duration,
) {
    loop {
        let available = request_queue.wait_available(None);
        if available {
            // If data is available to be polled, we wait for the batching duration, to allow accumulation, then poll.
            std::thread::sleep(batch_duration);
            if let Some(request) = request_queue.next() {
                // TODO: Request should be logged via telemetry here.
                if let Err(e) = request.handle_request(Arc::clone(inner_client)) {
                    error_support::warn!("ads-client background request failed: {e}");
                }
            }
        }
    }
}

// Request dispatch enum for passing different instructions to the background worker thread.
// `RequestAds` are prefetch mechanisms that query and load data into the local cache.
#[derive(Clone, Debug, PartialEq)]
pub enum DispatchRequest {
    RecordClick {
        url: Url,
    },
    RecordImpression {
        url: Url,
    },
    ReportAd {
        url: Url,
        reason: ReportReason,
    },
    RequestAds {
        ad_requests: HashSet<AdPlacementRequest>,
    },
}

impl DispatchRequest {
    // Runs a dispatched command synchronously in it's thread.
    // The dispatched command calls the corresponding `AdsClient` synchronous method, meaning that behavior between the two is shared.
    pub fn handle_request(
        self,
        ads_client_inner: Arc<AdsClientInner>,
    ) -> Result<(), ComponentError> {
        match self {
            // TODO: All of the MARS options are defaulted, as the AdsClient options refactor is required first.
            // These need to be passed when the request options refactor is merged.
            // This includes the `ohttp=false` flags, as well as `MARSRequestOptions::default()` in `prefetch_ads` below!
            // The same is true for relevance: `AdsRequestRelevance::new()`
            DispatchRequest::RecordClick { url } => ads_client_inner.record_click(url, false)?,
            DispatchRequest::ReportAd { url, reason } => {
                ads_client_inner.report_ad(url, reason, false)?
            }
            DispatchRequest::RecordImpression { url } => {
                ads_client_inner.record_impression(url, false)?
            }
            DispatchRequest::RequestAds { ad_requests } => {
                ads_client_inner.prefetch_ads_request(
                    ad_requests.into_iter().collect(),
                    MARSRequestOptions::default(),
                    AdRequestRelevance::new(),
                )?;
            }
        }
        Ok(())
    }
}

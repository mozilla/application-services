use error_support::handle_error;
use parking_lot::Mutex;
use url::Url;

use crate::{
    client::error::{BackgroundWorkerError, ComponentError},
    mars::{ad_request::AdPlacementRequest, ReportReason},
    request::{QueuedRequest, RequestQueue},
    AdsClientApiResult, MozAdsClientInner,
};
use std::{collections::HashSet, sync::Arc, thread::JoinHandle, time::Duration};

pub const ADS_CLIENT_WORKER_THREAD_NAME: &str = "ads-client.worker";

// Thread sleep delay to wait for new requests to queue before resolving them.
// This must be a very short amount of time to ensure ads are requested near-immediately on startup.
pub const ADS_CLIENT_WORKER_SLEEP_LENGTH: Duration = Duration::from_millis(2);

pub struct BackgroundWorker {
    _worker_thread: Option<JoinHandle<()>>,
    worker_request_queue: Option<Arc<Mutex<RequestQueue>>>,
}

impl BackgroundWorker {
    pub fn new(inner_client: MozAdsClientInner) -> BackgroundWorker {
        let request_queue = Arc::new(Mutex::new(RequestQueue::new()));
        let worker_request_queue = request_queue.clone();

        // Attempt to spawn a background worker thread to handle requests.
        // If it fails, all fields of `BackgroundWorker` will be populated with None, and an error will be logged.
        let Some(worker_thread) = std::thread::Builder::new()
            .name(ADS_CLIENT_WORKER_THREAD_NAME.to_string())
            .spawn(move || crate::worker::worker(inner_client, worker_request_queue.clone())).inspect_err(|err| {
                error_support::error!("Failed to create ads-client worker thread `{ADS_CLIENT_WORKER_THREAD_NAME}` with: {err}")
            }).ok()
        else {
            return BackgroundWorker { _worker_thread: None, worker_request_queue: None }
        };

        BackgroundWorker {
            _worker_thread: Some(worker_thread),
            worker_request_queue: Some(request_queue),
        }
    }

    pub fn new_empty() -> BackgroundWorker {
        BackgroundWorker {
            _worker_thread: None,
            worker_request_queue: None,
        }
    }

    // Add to the background worker's queue an ads request.
    // These are prioritized and will be batched before sending.
    pub fn dispatch_ads_request(
        &self,
        ads_requests: Vec<AdPlacementRequest>,
    ) -> Result<(), ComponentError> {
        if let Some(worker_dispatch) = &self.worker_request_queue {
            let mut queue = worker_dispatch.lock();
            for req in ads_requests {
                queue.push_ad_request(req);
            }
            Ok(())
        } else {
            Err(BackgroundWorkerError::Closed.into())
        }
    }

    // Add to the background worker's queue a QueuedRequest (a non-ads request).
    // (eg: RecordClick, RecordImpression)
    // These are queued to send after any ads requests.
    pub fn dispatch_queued_request(
        &self,
        queued_request: QueuedRequest,
    ) -> Result<(), ComponentError> {
        if let Some(worker_dispatch) = &self.worker_request_queue {
            let mut queue = worker_dispatch.lock();
            queue.push_queued_request(queued_request);
            Ok(())
        } else {
            Err(BackgroundWorkerError::Closed.into())
        }
    }
}

// Endless worker for background thread that synchronously run tasks in the order provided by the RequestQueue.
fn worker(inner_client: MozAdsClientInner, request_queue: Arc<Mutex<RequestQueue>>) {
    loop {
        // Get lock and extract next request. Lock is not held over the duration of the request but immediately dropped.
        let next_request = {
            let mut queue = request_queue.lock();
            queue.next()
        };

        if let Some(request) = next_request {
            // Error is already logged through `handle_error` conversion macro.
            let _ = request.handle_request(&inner_client);
        } else {
            // If the queue is empty, we have a very short pause with no locks to allow it to be written to.
            std::thread::sleep(ADS_CLIENT_WORKER_SLEEP_LENGTH);
        }
    }
}

// Request dispatch enum for passing different instructions to the background worker thread.
// `RequestAds` are prefetch mechanisms that query and load data into the local cache.
#[derive(Clone, Debug, PartialEq)]
pub enum DispatchRequest {
    RequestAds {
        ad_requests: HashSet<AdPlacementRequest>,
    },
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
}

impl DispatchRequest {
    // Runs a dispatched command synchronously in it's thread.
    // The dispatched command calls the corresponding `AdsClient` synchronous method, meaning that behavior between the two is shared.
    #[handle_error(ComponentError)]
    pub fn handle_request(self, _ads_client_inner: &MozAdsClientInner) -> AdsClientApiResult<()> {
        // TODO: Add 'running a request' logic to here.
        error_support::error!("Running a request is currently not set up yet: {self:?}");

        Ok(())
    }
}

use error_support::handle_error;
use parking_lot::Mutex;
use url::Url;

use crate::{
    client::error::{BackgroundWorkerError, ComponentError},
    mars::{ad_request::AdPlacementRequest, ReportReason},
    request::{QueuedRequest, RequestQueue},
    AdsClientApiResult, MozAdsClientInner,
};
use std::{collections::HashSet, sync::Arc, thread::JoinHandle};

pub const ADS_CLIENT_WORKER_THREAD_NAME: &str = "ads-client.worker";

pub struct BackgroundWorker {
    _worker_thread: Option<JoinHandle<()>>,
    worker_request_queue: Option<Arc<Mutex<RequestQueue>>>,
}

impl BackgroundWorker {
    pub fn new(inner_client: MozAdsClientInner) -> BackgroundWorker {
        let request_queue = Arc::new(Mutex::new(RequestQueue::new()));
        let worker_request_queue = request_queue.clone();

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
            // TODO: delay here?
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

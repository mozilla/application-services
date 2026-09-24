use url::Url;

use crate::mars::{ad_request::AdPlacementRequest, ReportReason};
use std::collections::VecDeque;

pub const MAXIMUM_ADS_BATCH_COUNT: usize = 100;

// Structure for storing and queuing and storing `DispatchRequest`s sent to background worker.
// AdPlacementRequest will be merged together, allowing fewer network requests to be sent.
pub struct RequestQueue {
    queued_ads: Vec<AdPlacementRequest>,

    // The ordered queue of non-AdPlacementRequest commands to iterate through.
    request_queue: VecDeque<QueuedRequest>,
}

impl RequestQueue {
    pub fn new() -> RequestQueue {
        RequestQueue {
            queued_ads: Vec::new(),
            request_queue: VecDeque::new(),
        }
    }
    pub fn push_ad_request(&mut self, ad_request: AdPlacementRequest) {
        self.queued_ads.push(ad_request);
    }

    pub fn push_queued_request(&mut self, queued_command: QueuedRequest) {
        self.request_queue.push_back(queued_command);
    }

    // Pops the next available DispatchRequest to be run by the worker.
    // This will prioritize any ad_requests, batched, followed by any queued QueuedRequests (eg: RecordClick, etc)
    pub fn next(&mut self) -> Option<DispatchRequest> {
        // First, if any ad requests are queued, batch the first `MAXIMUM_ADS_BATCH_COUNT` and resolve those.
        let num_ads = self.queued_ads.len().min(MAXIMUM_ADS_BATCH_COUNT);
        let ad_requests: Vec<_> = self.queued_ads.drain(..num_ads).collect();
        if ad_requests.len() > 0 {
            return Some(DispatchRequest::RequestAds { ad_requests });
        }

        // Otherwise, pop the next queue-able command.
        self.request_queue.pop_front().map(|c| c.into())
    }

    pub fn clear(&mut self) {
        self.request_queue = VecDeque::new();
        self.queued_ads = Vec::new();
    }
}

// Queue-able command (ReportAd, etc.)
#[derive(Debug, Clone, PartialEq)]
pub enum QueuedRequest {
    RecordClick { url: Url },
    RecordImpression { url: Url },
    ReportAd { url: Url, reason: ReportReason },
}

// Request dispatch enum for passing different instructions to the background worker thread.
// `RequestAds` are prefetch mechanisms that query and load data into the local cache.
#[derive(Clone, Debug, PartialEq)]
pub enum DispatchRequest {
    RequestAds {
        ad_requests: Vec<AdPlacementRequest>,
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

impl From<QueuedRequest> for DispatchRequest {
    fn from(value: QueuedRequest) -> Self {
        match value {
            QueuedRequest::ReportAd { url, reason } => DispatchRequest::ReportAd { url, reason },
            QueuedRequest::RecordClick { url } => DispatchRequest::RecordClick { url },
            QueuedRequest::RecordImpression { url } => DispatchRequest::RecordImpression { url },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::request::{
        AdPlacementRequest, DispatchRequest, RequestQueue, MAXIMUM_ADS_BATCH_COUNT,
    };

    fn example_request_ads() -> AdPlacementRequest {
        AdPlacementRequest {
            count: 4,
            placement: "test_placement".to_string(),
            content: None,
        }
    }

    fn example_request_ads_command() -> DispatchRequest {
        DispatchRequest::RequestAds {
            ad_requests: vec![AdPlacementRequest {
                count: 4,
                placement: "test_placement".to_string(),
                content: None,
            }],
        }
    }

    #[test]
    fn identical_command_comes_out() {
        let request = example_request_ads();
        let command = example_request_ads_command();
        let mut queue = RequestQueue::new();

        queue.push_ad_request(request.clone());
        let retrieved_command = queue.next().expect("Request should exist in queue");
        assert_eq!(command, retrieved_command);
    }

    #[test]
    fn batch_similar_requests() {
        let request = example_request_ads();
        let mut command = example_request_ads_command();
        let mut queue = RequestQueue::new();

        queue.push_ad_request(request.clone());
        queue.push_ad_request(request.clone());

        let retrieved_command = queue.next().expect("Request should exist in queue");
        assert!(queue.next().is_none());
        assert!(queue.queued_ads.is_empty());
        assert!(queue.request_queue.is_empty());

        // Modify `command` so it has more than one request inside.
        #[allow(irrefutable_let_patterns)]
        if let DispatchRequest::RequestAds {
            ref mut ad_requests,
            ..
        } = command
        {
            ad_requests.push(ad_requests[0].clone());
        } else {
            panic!("Example DispatchRequest should be RequestAds variant");
        };
        assert_eq!(command, retrieved_command);
    }

    #[test]
    fn split_off_too_many_ads() {
        let request = example_request_ads();
        let command = example_request_ads_command();
        let mut queue = RequestQueue::new();

        // Queue enough of this command to go one-over the limit.
        for _ in 0..(MAXIMUM_ADS_BATCH_COUNT + 1) {
            queue.push_ad_request(request.clone());
        }

        let retrieved_command = queue.next().expect("First command should exist in queue");
        let retrieved_command_overflow =
            queue.next().expect("Second command should exist in queue");

        assert!(queue.next().is_none());
        assert!(queue.queued_ads.is_empty());
        assert!(queue.request_queue.is_empty());

        assert_eq!(command, retrieved_command_overflow);

        // Retrieved command should have `MAXIMUM_ADS_BATCH_COUNT` requests.
        #[allow(irrefutable_let_patterns)]
        if let DispatchRequest::RequestAds {
            ref ad_requests, ..
        } = retrieved_command
        {
            assert_eq!(ad_requests.len(), MAXIMUM_ADS_BATCH_COUNT)
        } else {
            panic!("Example DispatchRequest should be RequestAds variant");
        };

        // Retrieved overflow should have 1 request, matching the original command.
        #[allow(irrefutable_let_patterns)]
        if let DispatchRequest::RequestAds {
            ref ad_requests, ..
        } = retrieved_command_overflow
        {
            assert_eq!(ad_requests.len(), 1)
        } else {
            panic!("Example DispatchRequest should be RequestAds variant");
        };
    }
}

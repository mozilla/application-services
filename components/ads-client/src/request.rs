use url::Url;

use crate::{
    ads::PlacementId,
    mars::{ad_request::AdPlacementRequest, ReportReason},
};
use std::collections::{HashMap, HashSet, VecDeque};

pub const MAXIMUM_ADS_BATCH_COUNT: usize = 100;

// Structure for storing and queuing and storing `DispatchRequest`s sent to background worker.
// AdPlacementRequest will be merged together, allowing fewer network requests to be sent.
pub struct RequestQueue {
    queued_ads: HashMap<PlacementId, AdPlacementRequest>,

    // The ordered queue of non-AdPlacementRequest commands to iterate through.
    request_queue: VecDeque<QueuedRequest>,
}

impl RequestQueue {
    pub fn new() -> RequestQueue {
        RequestQueue {
            queued_ads: HashMap::new(),
            request_queue: VecDeque::new(),
        }
    }

    pub fn push_ad_request(&mut self, ad_request: AdPlacementRequest) {
        self.queued_ads
            .insert(ad_request.placement.clone().into(), ad_request);
    }

    pub fn push_queued_request(&mut self, queued_command: QueuedRequest) {
        self.request_queue.push_back(queued_command);
    }

    // Pops the next available DispatchRequest to be run by the worker.
    // This will prioritize any ad_requests, batched, followed by any queued QueuedRequests (eg: RecordClick, etc)
    pub fn next(&mut self) -> Option<DispatchRequest> {
        // First, if any ad requests are queued, batch the first `MAXIMUM_ADS_BATCH_COUNT` and resolve those.
        let num_ads = self.queued_ads.len().min(MAXIMUM_ADS_BATCH_COUNT);
        let mut ad_requests: Vec<_> = self.queued_ads.drain().collect();
        if ad_requests.len() > 0 {
            let requeue_requests = ad_requests.split_off(num_ads);
            self.queued_ads = requeue_requests.into_iter().collect();
            return Some(DispatchRequest::RequestAds {
                ad_requests: ad_requests.into_iter().map(|(_, v)| v).collect(),
            });
        }

        // Otherwise, pop the next queue-able command.
        self.request_queue.pop_front().map(|c| c.into())
    }

    pub fn clear(&mut self) {
        self.request_queue = VecDeque::new();
        self.queued_ads = HashMap::new();
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
    use std::collections::HashSet;

    use crate::request::{
        AdPlacementRequest, DispatchRequest, RequestQueue, MAXIMUM_ADS_BATCH_COUNT,
    };

    fn extract_request_ads(req: &mut DispatchRequest) -> Option<&mut HashSet<AdPlacementRequest>> {
        #[allow(irrefutable_let_patterns)]
        if let DispatchRequest::RequestAds {
            ref mut ad_requests,
            ..
        } = req
        {
            Some(ad_requests)
        } else {
            None
        }
    }

    fn example_request_ads(identifier: usize) -> AdPlacementRequest {
        AdPlacementRequest {
            count: 4,
            placement: format!("test_placement_{identifier}"),
            content: None,
        }
    }

    fn example_request_ads_dispatch(identifiers: &[usize]) -> DispatchRequest {
        let mut ad_requests = HashSet::new();
        for id in identifiers {
            ad_requests.insert(AdPlacementRequest {
                count: 4,
                placement: format!("test_placement_{id}"),
                content: None,
            });
        }
        DispatchRequest::RequestAds { ad_requests }
    }

    #[test]
    fn identical_command_comes_out() {
        let request = example_request_ads(0);
        let command = example_request_ads_dispatch(&[0]);
        let mut queue = RequestQueue::new();

        queue.push_ad_request(request.clone());
        let retrieved_dispatch = queue.next().expect("Request should exist in queue");
        assert_eq!(command, retrieved_dispatch);
    }

    #[test]
    fn batch_similar_requests() {
        let request_0 = example_request_ads(0);
        let request_1 = example_request_ads(1);
        let dispatch = example_request_ads_dispatch(&[0, 1]);
        let mut queue = RequestQueue::new();

        // Having two "placement_0" and one "placement_1" should result in returning a single DispatchRequest with one "placement_0" and one "placement_1"
        queue.push_ad_request(request_0.clone());
        queue.push_ad_request(request_0.clone());
        queue.push_ad_request(request_1.clone());

        let retrieved_dispatch = queue.next().expect("Request should exist in queue");
        assert!(queue.next().is_none());
        assert!(queue.queued_ads.is_empty());
        assert!(queue.request_queue.is_empty());

        assert_eq!(dispatch, retrieved_dispatch);
    }

    #[test]
    fn split_off_too_many_ads() {
        let mut queue = RequestQueue::new();

        // Queue enough of this command to go one-over the limit.
        for i in 0..(MAXIMUM_ADS_BATCH_COUNT + 1) {
            let request = example_request_ads(i);
            queue.push_ad_request(request.clone());
        }

        let mut retrieved_dispatch_many =
            queue.next().expect("First command should exist in queue");
        let mut retrieved_dispatch_overflow =
            queue.next().expect("Second command should exist in queue");

        assert!(queue.next().is_none());
        assert!(queue.queued_ads.is_empty());
        assert!(queue.request_queue.is_empty());

        // We do not check for a precise match with sample data here, because HashMap<..>s are unsorted, so which placement gets pushed to the next one is random.
        // It is also irrelevant to the MARS request.
        // (So for example, if we have 101 requests, and a batch size of 100, the one excluded may be placement 1, placement 37, etc.)
        // Here, we specifically only check for the number of requests provided.
        // Retrieved command should have `MAXIMUM_ADS_BATCH_COUNT` requests.
        let retrieved_dispatch_many_ads = extract_request_ads(&mut retrieved_dispatch_many)
            .expect("Example DispatchRequest should be RequestAds variant");
        assert_eq!(retrieved_dispatch_many_ads.len(), MAXIMUM_ADS_BATCH_COUNT);

        // Retrieved overflow should have 1 request, matching the original command.
        let retrieved_dispatch_overflow_ads = extract_request_ads(&mut retrieved_dispatch_overflow)
            .expect("Example DispatchRequest should be RequestAds variant");
        assert_eq!(retrieved_dispatch_overflow_ads.len(), 1);
    }
}

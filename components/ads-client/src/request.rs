/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::worker::DispatchRequest;
use std::time::Duration;

pub const MAXIMUM_ADS_BATCH_COUNT: usize = 100;

pub mod batcher;
pub mod simple;

pub trait RequestQueue: Send + Sync {
    fn clear(&self);
    fn wait_available(&self, timeout: Option<Duration>) -> bool;
    fn next(&self) -> Option<DispatchRequest>;
    fn push_request(&self, request: DispatchRequest);

    fn wait_batch(&self);
    // TODO: Rename, and also reorganize these. (Only need to implement a few)
    // TODO: This is is an incomplete pattern.
    // TODO: Timeout only applies to
    fn wait_available_and_wait_batch(&self, timeout: Option<Duration>) -> bool {
        let b = self.wait_available(timeout);
        self.wait_batch();
        b
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, sync::Arc, time::Duration};
    use url::Url;

    use crate::{
        mars::ad_request::AdPlacementRequest,
        request::{
            batcher::RequestBatcher, simple::RequestSimpleQueue, DispatchRequest, RequestQueue,
        },
    };

    // TODO: reuse these in test_utils
    fn example_request_ads_dispatch(identifiers: &[usize]) -> DispatchRequest {
        let mut ad_requests = HashSet::new();
        for id in identifiers {
            ad_requests.insert(AdPlacementRequest {
                content: None,
                count: 4,
                placement: format!("test_placement_{id}"),
            });
        }
        DispatchRequest::RequestAds { ad_requests }
    }

    #[test]
    fn identical_command_comes_out() {
        #[track_caller]
        fn test_function(queue: Arc<dyn RequestQueue>) {
            let request = example_request_ads_dispatch(&[0]);
            let command = example_request_ads_dispatch(&[0]);
            queue.push_request(request.clone());
            let retrieved_dispatch = queue.next().expect("Request should exist in queue");
            assert_eq!(command, retrieved_dispatch);
        }
        test_function(Arc::new(RequestSimpleQueue::new()));
        test_function(Arc::new(RequestBatcher::new(None)));
    }

    #[test]
    fn waiting_if_available_returns_successfully_push_queued_request() {
        #[track_caller]
        fn test_function(queue: Arc<dyn RequestQueue>) {
            let queue_clone = queue.clone();

            // Check immediate result for `wait_available`
            queue_clone.push_request(DispatchRequest::RecordClick {
                url: Url::parse("https://example.net").unwrap(),
            });
            let available = queue.wait_available(Some(Duration::from_secs(2)));
            assert!(available);
            assert!(queue.next().is_some());

            // Check result in `wait_available` added by another thread while waiting.
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(1));
                println!("Pushing request.");
                queue_clone.push_request(DispatchRequest::RecordClick {
                    url: Url::parse("https://example.net").unwrap(),
                });
            });

            assert_eq!(queue.next(), None);
            println!("Waiting for lock.");
            let available = queue.wait_available(Some(Duration::from_secs(2)));
            assert!(available);
            assert!(queue.next().is_some());
            assert!(queue.next().is_none());
        }

        test_function(Arc::new(RequestSimpleQueue::new()));
        test_function(Arc::new(RequestBatcher::new(None)));
    }

    #[test]
    fn waiting_if_available_returns_successfully_push_ad_request() {
        #[track_caller]
        fn test_function(queue: Arc<dyn RequestQueue>) {
            let queue_clone = queue.clone();

            // Check immediate result for `wait_available`
            queue_clone.push_request(example_request_ads_dispatch(&[1]));
            let available = queue.wait_available(Some(Duration::from_secs(2)));
            assert!(available);
            assert!(queue.next().is_some());

            // Check result in `wait_available` added by another thread while waiting.
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(1));
                println!("Pushing request.");
                queue_clone.push_request(example_request_ads_dispatch(&[1]));
            });

            assert_eq!(queue.next(), None);
            println!("Waiting for lock.");
            let available = queue.wait_available(Some(Duration::from_secs(2)));
            assert!(available);
            assert!(queue.next().is_some());
            assert!(queue.next().is_none());
        }

        test_function(Arc::new(RequestSimpleQueue::new()));
        test_function(Arc::new(RequestBatcher::new(None)));
    }

    #[test]
    fn waiting_if_available_returns_unavailable() {
        #[track_caller]
        fn test_function(queue: Arc<dyn RequestQueue>) {
            // Check immediate result for `wait_available`
            let available = queue.wait_available(Some(Duration::from_secs(2)));
            assert!(!available);
            assert!(queue.next().is_none());
        }

        test_function(Arc::new(RequestSimpleQueue::new()));
        test_function(Arc::new(RequestBatcher::new(None)));
    }
}

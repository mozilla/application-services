/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::worker::DispatchRequest;
use std::time::Duration;

pub const MAXIMUM_ADS_BATCH_COUNT: usize = 100;

pub mod batcher;
pub mod simple;

/// Generic structure for queueing `DispatchRequest`s for the background worker.
/// You can append a request to the queue with `push_request(request)` and pop the next one with `next()`.
/// Inside the queue, queries might be batched together, depending on the implementation.
pub trait RequestQueue: Send + Sync {
    /// Clears all requests from the queue.
    /// Does not trigger `wait_available`.
    fn clear(&self);

    /// Fetches the next `DispatchRequest` from the queue.
    /// This is possibly a batch of requests combined into one `DispatchRequest.`
    fn next(&self) -> Option<DispatchRequest>;

    /// Push a request into the queue.
    fn push_request(&self, request: DispatchRequest);

    /// Waits a defined period to gather requests to build a batch. This sleeps the calling thread.
    /// This is to be used after the queue is readied, to allow for a period of time to gather requests for batching.
    /// (Without this, adding one request to an empty queue would result in an initial batch of size=1).
    fn wait_gather(&self);

    /// Wait for queue to have data retrievable by `next()`, with an optional timeout.
    /// This should use a condvar internally to ensure a thread can wait with minimal CPU usage.
    /// If multiple threads call `wait_available`, all will be notified on availability (so, competing threads may not all receive data from `next()`)
    /// Returns `true` if data is available to be queried by `next()`, and false otherwise (eg: if timeout is reached)
    fn wait_available(&self, timeout: Option<Duration>) -> bool;
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};
    use url::Url;

    use crate::{
        request::{
            batcher::RequestBatcher, simple::RequestSimpleQueue, DispatchRequest, RequestQueue,
        },
        test_utils::get_example_request_ads_dispatch_request,
    };

    #[test]
    fn identical_command_comes_out() {
        #[track_caller]
        fn test_function(queue: Arc<dyn RequestQueue>) {
            let request = get_example_request_ads_dispatch_request(&[0]);
            let command = get_example_request_ads_dispatch_request(&[0]);
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
            queue_clone.push_request(get_example_request_ads_dispatch_request(&[1]));
            let available = queue.wait_available(Some(Duration::from_secs(2)));
            assert!(available);
            assert!(queue.next().is_some());

            // Check result in `wait_available` added by another thread while waiting.
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(1));
                println!("Pushing request.");
                queue_clone.push_request(get_example_request_ads_dispatch_request(&[1]));
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

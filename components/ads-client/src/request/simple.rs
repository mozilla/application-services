/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{request::RequestQueue, worker::DispatchRequest};
use parking_lot::{Condvar, Mutex};
use std::{collections::VecDeque, sync::Arc, time::Duration};

pub const MAXIMUM_ADS_BATCH_COUNT: usize = 100;

// Structure for storing and queuing `DispatchRequest`s sent to background worker.
// It functions as a basic queue (FIFO), with no batching or special handling based on the type of request.
#[derive(Clone)]
pub struct RequestSimpleQueue {
    // If the `inner` mutex is locked to add data to the queue, the ready_cond must be notified after data is added.
    queue: Arc<Mutex<VecDeque<DispatchRequest>>>,
    // Condvar is updated if new data is queued to be retrievable by `next()`. Can be waited with: `wait_available`
    ready_cond: Arc<Condvar>,
}

impl Default for RequestSimpleQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestQueue for RequestSimpleQueue {
    fn clear(&self) {
        // Get lock and clear data. No need to update 'ready' condvar, as this can't result in more data.
        let mut queue = self.queue.lock();
        queue.clear();
    }

    // Wait for queue to have data retrievable by `next()`, with an optional timeout.
    // Uses a condvar internally to ensure a thread can wait with minimal CPU usage.
    // If multiple threads call `wait_available`, all will be notified on availability (so, competing threads may not all receive data from `next()`)
    // Returns `true` if data is available to be queried by `next()`, and false otherwise (eg: if timeout is reached)
    fn wait_available(&self, timeout: Option<Duration>) -> bool {
        let mut waiting = self.queue.lock();

        // Return immediately if data is currently available
        if !waiting.is_empty() {
            return true;
        }

        if let Some(timeout) = timeout {
            if self.ready_cond.wait_for(&mut waiting, timeout).timed_out() {
                return false;
            }
        } else {
            self.ready_cond.wait(&mut waiting);
        }
        !waiting.is_empty()
    }

    // Pops the next available DispatchRequest to be run by the worker.
    fn next(&self) -> Option<DispatchRequest> {
        let mut queue = self.queue.lock();
        queue.pop_front().map(Into::into)
    }

    // Add a `DispatchRequest` to the  queue and notify condvar.
    fn push_request(&self, request: DispatchRequest) {
        let mut queue = self.queue.lock();
        queue.push_back(request);

        // Queue was written to, so we notify any threads with `wait_available`.
        self.ready_cond.notify_all();
    }

    // `wait_batch` is a noop, as no batching happens here.
    fn wait_batch(&self) {}
}

impl RequestSimpleQueue {
    pub fn new() -> RequestSimpleQueue {
        RequestSimpleQueue {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            ready_cond: Arc::new(Condvar::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        request::{simple::RequestSimpleQueue, RequestQueue},
        worker::DispatchRequest,
    };

    #[test]
    fn commands_queued_in_fifo_order() {
        let queue = RequestSimpleQueue::new();
        let request_0 = DispatchRequest::RecordClick {
            url: "http://dummy_url_0.com".try_into().unwrap(),
        };
        let request_1 = DispatchRequest::RecordClick {
            url: "http://dummy_url_1.com".try_into().unwrap(),
        };
        queue.push_request(request_0.clone());
        queue.push_request(request_1.clone());

        let retrieved_dispatch = queue.next().expect("Request should exist in queue");
        assert_eq!(request_0, retrieved_dispatch);
        let retrieved_dispatch = queue.next().expect("Request should exist in queue");
        assert_eq!(request_1, retrieved_dispatch);
    }
}

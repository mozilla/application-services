/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use parking_lot::{Condvar, Mutex};
use url::Url;

use crate::{
    ads::PlacementId,
    mars::{ad_request::AdPlacementRequest, ReportReason},
    request::RequestQueue,
    worker::DispatchRequest,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};

pub const MAXIMUM_ADS_BATCH_COUNT: usize = 100;

// Structure for storing and queuing and storing `DispatchRequest`s sent to background worker.
// AdPlacementRequest will be merged together, allowing fewer network requests to be sent.
// TODO: Redo all the documentation in the file.
// TODO: Rename from this.
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
        // TODO: Can you simplify this so only one requires it?
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
    // This will prioritize any ad_requests, batched, followed by any queued QueuedRequests (eg: RecordClick, etc)
    fn next(&self) -> Option<DispatchRequest> {
        let mut queue = self.queue.lock();
        queue.pop_front().map(Into::into)
    }

    // Add a `DispatchRequest` to the `RequestBatcher` (primary entry point for queuing requests).
    // If new helper functions are added, they must include a convdvar notification.
    fn push_request(&self, request: DispatchRequest) {
        let mut queue = self.queue.lock();
        queue.push_back(request);

        // Queue was written to, so we notify any threads with `wait_available`.
        self.ready_cond.notify_all();
    }
}

impl RequestSimpleQueue {
    pub fn new() -> RequestSimpleQueue {
        RequestSimpleQueue {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            ready_cond: Arc::new(Condvar::new()),
        }
    }
}

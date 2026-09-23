/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::sync::Arc;

use context_id::ContextIdCallback;
use parking_lot::Mutex;

/// Collects the context ids retired by `ContextIDComponent` rotations so
/// `AdsClient` can send their deletion requests itself: outside the
/// component's lock and over the transport the triggering ad request used.
///
/// Clones share one queue. One clone is boxed into the component as its
/// callback, the other stays on the client. `persist` is a no-op because the
/// ads client does not persist its context id.
#[derive(Clone, Debug, Default)]
pub struct ContextIdDeletionQueue(Arc<Mutex<Vec<String>>>);

impl ContextIdDeletionQueue {
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.0.lock().is_empty()
    }

    /// Drains every queued id, leaving the queue empty.
    pub fn take_all(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock())
    }
}

impl ContextIdCallback for ContextIdDeletionQueue {
    fn persist(&self, _context_id: String, _creation_date: i64) {}

    fn rotated(&self, old_context_id: String) {
        self.0.lock().push(old_context_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use context_id::ContextIdCallback;

    #[test]
    fn test_rotated_ids_are_queued_until_taken() {
        let queue = ContextIdDeletionQueue::default();
        assert!(queue.is_empty());

        queue.rotated("old-1".to_string());
        queue.rotated("old-2".to_string());
        assert!(!queue.is_empty());

        assert_eq!(
            queue.take_all(),
            vec!["old-1".to_string(), "old-2".to_string()]
        );
        assert!(queue.is_empty());
        assert!(queue.take_all().is_empty());
    }

    #[test]
    fn test_persist_is_a_no_op() {
        let queue = ContextIdDeletionQueue::default();
        queue.persist("id".to_string(), 0);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_clones_share_the_queue() {
        let queue = ContextIdDeletionQueue::default();
        let callback = queue.clone();
        callback.rotated("old".to_string());
        assert_eq!(queue.take_all(), vec!["old".to_string()]);
    }
}

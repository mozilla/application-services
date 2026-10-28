/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

use std::sync::Arc;

use context_id::ContextIdCallback;
use parking_lot::Mutex;

/// `ContextIdCallback` that queues retired context ids. The component calls
/// `rotated` under its own lock and without the request's OHTTP flag, so
/// `AdsClient` drains the queue in `request_ads` and sends the deletions itself.
///
/// In practice the queue holds at most one id per flush: `rotated` only fires
/// from the `request` call inside `request_ads`, and a rotation resets the
/// context id's creation timestamp, so two rotations are at least
/// `DEFAULT_ROTATION_DAYS` apart. It is a `Vec` rather than an `Option` only
/// because the `ContextIdCallback` contract does not promise that, and a `Vec`
/// never drops an id.
///
/// Clones share one queue. `persist` is a no-op because the ads client does
/// not persist its context id.
#[derive(Clone, Debug, Default)]
pub struct ContextIdDeletionQueue(Arc<Mutex<Vec<String>>>);

impl ContextIdDeletionQueue {
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.0.lock().is_empty()
    }

    pub fn drain(&self) -> Vec<String> {
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
    fn test_rotated_ids_are_queued_until_drained() {
        let queue = ContextIdDeletionQueue::default();
        assert!(queue.is_empty());

        queue.rotated("old-1".to_string());
        queue.rotated("old-2".to_string());
        assert!(!queue.is_empty());

        assert_eq!(
            queue.drain(),
            vec!["old-1".to_string(), "old-2".to_string()]
        );
        assert!(queue.is_empty());
        assert!(queue.drain().is_empty());
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
        assert_eq!(queue.drain(), vec!["old".to_string()]);
    }
}

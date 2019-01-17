/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::*;
use rusqlite::InterruptHandle;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

// SeqCst is overkill for much of this, but whatever.

/// A Sync+Send type which can be used allow someone to interrupt an
/// operation, even if it happens while rust code (and not SQL) is
/// executing.
pub struct PlacesInterruptHandle {
    pub(crate) db_handle: InterruptHandle,
    pub(crate) interrupt_counter: Arc<AtomicUsize>,
}

impl PlacesInterruptHandle {
    pub fn interrupt(&self) {
        self.interrupt_counter.fetch_add(1, Ordering::SeqCst);
        self.db_handle.interrupt();
    }
}

/// A helper that can be used to determine if an interrupt request has come in while
/// the object lives. This is used to avoid a case where we aren't running any
/// queries when the request to stop comes in, but we're still not done (for example,
/// maybe we've run some of the autocomplete matchers, and are about to start
/// running the others. If we rely solely on sqlite3_interrupt(), we'd miss
/// the message that we should stop).
pub(crate) struct InterruptScope {
    // The value of the interrupt counter when the scope began
    start_value: usize,
    // This could be &'conn AtomicUsize, but it would prevent the connection
    // from being mutably borrowed for no real reason...
    ptr: Arc<AtomicUsize>,
}

impl InterruptScope {
    #[inline]
    pub(crate) fn new(ptr: Arc<AtomicUsize>) -> Self {
        let start_value = ptr.load(Ordering::SeqCst);
        Self { start_value, ptr }
    }

    #[inline]
    pub(crate) fn was_interrupted(&self) -> bool {
        self.ptr.load(Ordering::SeqCst) != self.start_value
    }

    #[inline]
    pub(crate) fn err_if_interrupted(&self) -> Result<()> {
        if self.was_interrupted() {
            Err(ErrorKind::InterruptedError.into())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_sync_send() {
        fn is_sync<T: Sync>() {}
        fn is_send<T: Send>() {}
        // Make sure this compiles
        is_sync::<PlacesInterruptHandle>();
        is_send::<PlacesInterruptHandle>();
    }
}

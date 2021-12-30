/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{Interrupted, Interruptee};
use std::sync::atomic::{AtomicUsize, Ordering};

// Shared counter for InterruptScope:
//   - The `interrupt()` method increments this.
//   - The `was_interrupted()` method checks if this was incremented since the `InterruptScope` was
//     created.
static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Scope for an interruptable operations
#[derive(Clone, Debug)]
pub struct InterruptScope {
    start_value: usize,
}

/// Scope for interruptible operations
///
/// This struct provides interruption support for syncing and other long-running operations.  The
/// basic system is:
///
///    - Call `InterruptScope::new()` to create a new `InterruptScope` at the begining of the
///      operation.  If multiple components are involved in the same operation, then they should
///      share clones of a single `InterruptScope`.  For example `SyncManager.sync()` creates an
///      interrupt scope that all `SyncEngine` implementations share.
///    - During the operation, regularly check if the scope is interrupted with `was_interrupted()`
///      or `err_if_interrupted()` which is often more ergonomic.
///    - Call `InterruptScope::interrupt_current_scopes()` to interrupt all previously created
///      `InterruptScope`s. `InterruptScope`s created after this call will not be considerid
///      interrupted unless `InterruptScope::interrupt()` is called again.
///
/// This type requires the code to actively if it's interrupted.  Therefore:
///   - Make sure to sprinkle in `err_if_interrupted()` calls inside the code you want
///     interruptable.  Loops are a particularly good place to put these.
///   - This cannot interrupt external code.  In particular, it won't interrupt a long-running SQLite
///     query.  Use an `rusqlite::InterruptHandler` for that.
///
impl InterruptScope {
    // In order to ensure these functions are fast:
    //
    //   - All functions are inlined, since they are called from different crates.
    //   - We use Ordering::Relaxed since all we need is an atomic check.

    /// Create a new `InterruptScope`
    #[inline]
    pub fn new() -> Self {
        Self {
            start_value: COUNTER.load(Ordering::Relaxed),
        }
    }

    /// Interrupt any `InterruptScope`s created before this call.
    #[inline]
    pub fn interrupt() {
        COUNTER.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    /// Check if this this scope was interrupted
    fn was_interrupted(&self) -> bool {
        COUNTER.load(Ordering::Relaxed) != self.start_value
    }

    #[inline]
    /// Return Err(Interrupted) if this scope was interrupted
    pub fn err_if_interrupted(&self) -> Result<(), Interrupted> {
        if self.was_interrupted() {
            Err(Interrupted)
        } else {
            Ok(())
        }
    }
}

impl Default for InterruptScope {
    fn default() -> Self {
        Self::new()
    }
}

// Needed to make the old sync_multiple code work.  Let's remove this once all syncing goes through
// SyncManager
impl Interruptee for InterruptScope {
    fn was_interrupted(&self) -> bool {
        InterruptScope::was_interrupted(self)
    }
}

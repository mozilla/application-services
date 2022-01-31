/* This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use parking_lot::Mutex;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Weak;

static IN_SHUTDOWN: AtomicBool = AtomicBool::new(false);
lazy_static::lazy_static! {
   static ref REGISTERED_INTERRUPTS: Mutex<Vec<Weak<dyn ShutdownInterrupt>>> = Mutex::new(Vec::new());
}

/// Initiate shutdown:
///
/// - All registered `ShutdownInterrupt` instances will have their `interrupt()` methods called.
/// - After this `err_if_shutdown` will always return a ShutdownError
pub fn shutdown() {
    IN_SHUTDOWN.store(true, Ordering::Relaxed);
    for weak in REGISTERED_INTERRUPTS.lock().iter() {
        if let Some(interrupt) = weak.upgrade() {
            interrupt.interrupt()
        }
    }
}

/// Restart after a shutdown
///
/// - After this `err_if_shutdown` will no longer return a ShutdownError
pub fn restart() {
    IN_SHUTDOWN.store(false, Ordering::Relaxed);
}

/// Check if we're currently in shutdown mode
pub fn in_shutdown() -> bool {
    IN_SHUTDOWN.load(Ordering::Relaxed)
}

/// Return a ShutdownError if start_shutdown() has been called
pub fn err_if_shutdown() -> Result<(), ShutdownError> {
    if in_shutdown() {
        Err(ShutdownError)
    } else {
        Ok(())
    }
}

/// The error returned by err_if_shutdown
#[derive(Debug, Clone, PartialEq)]
pub struct ShutdownError;

impl fmt::Display for ShutdownError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Application is shutting down")
    }
}

impl std::error::Error for ShutdownError {}

/// Trait to handle interruption at shutdown
///
/// The main use case this trait targets is ensuring that sqlite queries are interrupted when
/// shutdown starts.  Usage:
///
///  - Each component implements `ShutdownInterrupt` on the Store struct that manages DB
///    connections.
///  - When those types get created, the components call `register_interrupt()`.
///  - When `start_shutdown()` is called, each registered `ShutdownInterrupt` instance will have
///    it's `interrupt()` method called
///  - `interrupt()` should interrupt all open DB connections.
pub trait ShutdownInterrupt: Send + Sync {
    fn interrupt(&self);
}

/// Register a ShutdownInterrupt implementation
///
/// Call this function to ensure that your `interrupt()` function will be called at shutdown.
pub fn register_interrupt(interrupt: Weak<dyn ShutdownInterrupt>) {
    // Note: we push a weak ref to REGISTERED_INTERRUPTS, but don't ever check if it's still alive.
    // This is fine for our current usage where we only create a limited number of Stores that
    // implement ShutdownInterrupt.  But if that changes, we should update this code to ensure that
    // REGISTERED_INTERRUPTS doesn't grow without bound.
    REGISTERED_INTERRUPTS.lock().push(interrupt);
}

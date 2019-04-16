/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Helps manage "interruptable" things across our various crates.

use failure::Fail;

/// Something that is interruptable. In practice, this will almost certainly
/// be Sync + Send, as it will typically be created on one thread, but
/// `.interrupt()` will be called from a different thread. However,
/// Sync + Send semantics aren't mandated here.
pub trait Interruptable {
    /// Take some action when interrupted.
    fn interrupt(&self);
}

/// Represents the state of something that may be interrupted. Decoupled from
/// Interruptable so that things which want to check if they have been
/// interrupted don't need to know about the interrupt mechanics.
pub trait Interruptee {
    fn was_interrupted(&self) -> bool;

    fn err_if_interrupted(&self) -> std::result::Result<(), Interrupted> {
        if self.was_interrupted() {
            Err(Interrupted)?
        } else {
            Ok(())
        }
    }
}

/// A convenience implementation, should only be used in tests.
pub struct NeverInterrupts;

impl Interruptee for NeverInterrupts {
    #[inline]
    fn was_interrupted(&self) -> bool {
        false
    }
}

/// The error returned by err_if_interrupted.
#[derive(Debug, Fail)]
#[fail(display = "The operation was interrupted.")]
pub struct Interrupted;

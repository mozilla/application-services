/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module contains code to dry-run test the state machine in against the existing Android/iOS implementations.
//! The idea is to create a `FxaStateChecker` instance, manually drive the state transitions and
//! check its state against the FirefoxAccount calls the existing implementation makes.

use super::logic::next_state;
use super::types::{Event, State};
use crate::FxaState;
use error_support::{breadcrumb, report_error};
use parking_lot::Mutex;

pub struct FxaStateMachineChecker {
    inner: Mutex<FxaStateMachineCheckerInner>,
}

struct FxaStateMachineCheckerInner {
    // Public state that the state machine is in.
    // This is the state that `FxaStateMachine` would have been in before `process_event` was called.
    current_state: FxaState,
    // Internal state that the state machine is in.
    // This is what `process_event` would have had in its loop.
    state: State,
    // Did we report an error?  If so, then we should give up checking things since the error is
    // likely to cascade
    reported_error: bool,
}

impl Default for FxaStateMachineChecker {
    fn default() -> Self {
        Self {
            inner: Mutex::new(FxaStateMachineCheckerInner {
                current_state: FxaState::Uninitialized,
                state: State::Uninitialized,
                reported_error: false,
            }),
        }
    }
}

impl FxaStateMachineChecker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance the internal state
    ///
    /// Call this when:
    /// - A top-level event is passed to `processQueue`/`processEvent`
    /// - A FirefoxAccount call finishes (either successfully or if it throws)
    pub fn advance(&self, event: Event) {
        let mut inner = self.inner.lock();
        if inner.reported_error {
            return;
        }
        let cloned_event = event.clone();
        match next_state(inner.state.clone(), event, &inner.current_state) {
            Ok(state) => {
                breadcrumb!("fxa-state-machine-checker: {cloned_event} -> {state}");
                inner.state = state;
            }
            Err(e) => {
                report_error!("fxa-state-machine-checker", "Error in advance: {e}");
                inner.reported_error = true;
            }
        }
    }

    /// Check the internal state
    ///
    /// Call this when:
    /// - A FirefoxAccount call is about to be made
    /// - `processQueue`/`processEvent` have advanced the existing state machine to a public state.
    pub fn check_state(&self, state: State) {
        let mut inner = self.inner.lock();
        if inner.reported_error {
            return;
        }
        if inner.state != state {
            report_error!(
                "fxa-state-machine-checker",
                "State mismatch: {} vs {state}",
                inner.state
            );
            inner.reported_error = true;
        }
    }
}

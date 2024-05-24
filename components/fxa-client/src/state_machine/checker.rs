/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This module contains code to dry-run test the state machine in against the existing Android/iOS implementations.
//! The idea is to create a `FxaStateChecker` instance, manually drive the state transitions and
//! check its state against the FirefoxAccount calls the existing implementation makes.
//!
//! Initially this will be tested by devs / QA.  Then we will ship it to real users and monitor which errors come back in Sentry.

use crate::{FxaEvent, FxaState};
use error_support::{breadcrumb, report_error};
use parking_lot::Mutex;

pub use super::internal_machines::Event as FxaStateCheckerEvent;
use super::internal_machines::State as InternalState;
use super::internal_machines::*;

/// State passed to the state checker, this is exactly the same as `internal_machines::State`
/// except the `Complete` variant uses a named field for UniFFI compatibility.
pub enum FxaStateCheckerState {
    GetAuthState,
    BeginOAuthFlow {
        scopes: Vec<String>,
        entrypoint: String,
    },
    BeginPairingFlow {
        pairing_url: String,
        scopes: Vec<String>,
        entrypoint: String,
    },
    CompleteOAuthFlow {
        code: String,
        state: String,
    },
    InitializeDevice,
    EnsureDeviceCapabilities,
    CheckAuthorizationStatus,
    GetProfile,
    Disconnect,
    Complete {
        new_state: FxaState,
    },
    Cancel,
}

pub struct FxaStateMachineChecker {
    inner: Mutex<FxaStateMachineCheckerInner>,
}

struct FxaStateMachineCheckerInner {
    public_state: FxaState,
    internal_state: InternalState,
    state_machine: Box<dyn InternalStateMachine + Send>,
    // Did we report an error?  If so, then we should give up checking things since the error is
    // likely to cascade
    reported_error: bool,
}

impl Default for FxaStateMachineChecker {
    fn default() -> Self {
        Self {
            inner: Mutex::new(FxaStateMachineCheckerInner {
                public_state: FxaState::Uninitialized,
                internal_state: InternalState::Cancel,
                state_machine: Box::new(UninitializedStateMachine),
                reported_error: false,
            }),
        }
    }
}

impl FxaStateMachineChecker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance the internal state based on a public event
    pub fn handle_public_event(&self, event: FxaEvent) {
        let mut inner = self.inner.lock();
        if inner.reported_error {
            return;
        }
        match &inner.internal_state {
            InternalState::Complete(_) | InternalState::Cancel => (),
            internal_state => {
                report_error!(
                    "fxa-state-machine-checker",
                    "handle_public_event called with non-terminal internal state (event: {event}, internal state: {internal_state})",
                 );
                inner.reported_error = true;
                return;
            }
        }

        inner.state_machine = make_state_machine(&inner.public_state);
        match inner.state_machine.initial_state(event.clone()) {
            Ok(state) => {
                breadcrumb!(
                    "fxa-state-machine-checker: public transition start ({event} -> {state})"
                );
                inner.internal_state = state;
                if let InternalState::Complete(new_state) = &inner.internal_state {
                    inner.public_state = new_state.clone();
                    breadcrumb!("fxa-state-machine-checker: public transition end");
                }
            }
            Err(e) => {
                report_error!(
                    "fxa-state-machine-checker",
                    "Error in handle_public_event: {e}"
                );
                inner.reported_error = true;
            }
        }
    }

    /// Advance the internal state based on an internal event
    pub fn handle_internal_event(&self, event: FxaStateCheckerEvent) {
        let mut inner = self.inner.lock();
        if inner.reported_error {
            return;
        }
        match inner
            .state_machine
            .next_state(inner.internal_state.clone(), event.clone())
        {
            Ok(state) => {
                breadcrumb!("fxa-state-machine-checker: internal transition ({event} -> {state})");
                match &state {
                    InternalState::Complete(new_state) => {
                        inner.public_state = new_state.clone();
                        breadcrumb!("fxa-state-machine-checker: public transition end");
                    }
                    InternalState::Cancel => {
                        breadcrumb!("fxa-state-machine-checker: public transition end (cancelled)");
                    }
                    _ => (),
                };
                inner.internal_state = state;
            }
            Err(e) => {
                report_error!(
                    "fxa-state-machine-checker",
                    "Error in handle_internal_event: {e}"
                );
                inner.reported_error = true;
            }
        }
    }

    /// Check the internal state
    ///
    /// Call this when `processQueue`/`processEvent` has advanced the existing state machine to a public state.
    pub fn check_public_state(&self, state: FxaState) {
        let mut inner = self.inner.lock();
        if inner.reported_error {
            return;
        }
        match &inner.internal_state {
            InternalState::Complete(_) | InternalState::Cancel => (),
            internal_state => {
                report_error!(
                    "fxa-state-machine-checker",
                    "check_public_state called with non-terminal internal state (expected: {state} actual internal state: {internal_state})"
                 );
                inner.reported_error = true;
                return;
            }
        }
        if inner.public_state != state {
            report_error!(
                "fxa-state-machine-checker",
                "Public state mismatch: expected: {state}, actual: {} ({})",
                inner.public_state,
                inner.internal_state
            );
            inner.reported_error = true;
        } else {
            breadcrumb!("fxa-state-machine-checker: check_public_state successful {state}");
        }
    }

    /// Check the internal state
    ///
    /// Call this when a FirefoxAccount call is about to be made
    pub fn check_internal_state(&self, state: FxaStateCheckerState) {
        let mut inner = self.inner.lock();
        if inner.reported_error {
            return;
        }
        let state: InternalState = state.into();
        if inner.internal_state != state {
            report_error!(
                "fxa-state-machine-checker",
                "Internal state mismatch (expected: {state}, actual: {})",
                inner.internal_state
            );
            inner.reported_error = true;
        } else {
            breadcrumb!("fxa-state-machine-checker: check_internal_state successful {state}");
        }
    }
}

fn make_state_machine(public_state: &FxaState) -> Box<dyn InternalStateMachine + Send> {
    match public_state {
        FxaState::Uninitialized => Box::new(UninitializedStateMachine),
        FxaState::Disconnected => Box::new(DisconnectedStateMachine),
        FxaState::Authenticating { .. } => Box::new(AuthenticatingStateMachine),
        FxaState::Connected => Box::new(ConnectedStateMachine),
        FxaState::AuthIssues => Box::new(AuthIssuesStateMachine),
    }
}

impl From<InternalState> for FxaStateCheckerState {
    fn from(state: InternalState) -> Self {
        match state {
            InternalState::GetAuthState => Self::GetAuthState,
            InternalState::BeginOAuthFlow { scopes, entrypoint } => {
                Self::BeginOAuthFlow { scopes, entrypoint }
            }
            InternalState::BeginPairingFlow {
                pairing_url,
                scopes,
                entrypoint,
            } => Self::BeginPairingFlow {
                pairing_url,
                scopes,
                entrypoint,
            },
            InternalState::CompleteOAuthFlow { code, state } => {
                Self::CompleteOAuthFlow { code, state }
            }
            InternalState::InitializeDevice => Self::InitializeDevice,
            InternalState::EnsureDeviceCapabilities => Self::EnsureDeviceCapabilities,
            InternalState::CheckAuthorizationStatus => Self::CheckAuthorizationStatus,
            InternalState::Disconnect => Self::Disconnect,
            InternalState::GetProfile => Self::GetProfile,
            InternalState::Complete(new_state) => Self::Complete { new_state },
            InternalState::Cancel => Self::Cancel,
        }
    }
}

impl From<FxaStateCheckerState> for InternalState {
    fn from(state: FxaStateCheckerState) -> Self {
        match state {
            FxaStateCheckerState::GetAuthState => Self::GetAuthState,
            FxaStateCheckerState::BeginOAuthFlow { scopes, entrypoint } => {
                Self::BeginOAuthFlow { scopes, entrypoint }
            }
            FxaStateCheckerState::BeginPairingFlow {
                pairing_url,
                scopes,
                entrypoint,
            } => Self::BeginPairingFlow {
                pairing_url,
                scopes,
                entrypoint,
            },
            FxaStateCheckerState::CompleteOAuthFlow { code, state } => {
                Self::CompleteOAuthFlow { code, state }
            }
            FxaStateCheckerState::InitializeDevice => Self::InitializeDevice,
            FxaStateCheckerState::EnsureDeviceCapabilities => Self::EnsureDeviceCapabilities,
            FxaStateCheckerState::CheckAuthorizationStatus => Self::CheckAuthorizationStatus,
            FxaStateCheckerState::Disconnect => Self::Disconnect,
            FxaStateCheckerState::GetProfile => Self::GetProfile,
            FxaStateCheckerState::Complete { new_state } => Self::Complete(new_state),
            FxaStateCheckerState::Cancel => Self::Cancel,
        }
    }
}

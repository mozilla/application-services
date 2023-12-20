/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! FxA state machine
//!
//! This presents a high-level API for logging in, logging out, dealing with authentication token issues, etc.

use error_support::breadcrumb;

use crate::{internal::FirefoxAccount, DeviceConfig, Error, FxaEvent, FxaState, Result};

pub mod checker;
mod display;
mod internal_machines;

/// Number of state transitions to perform before giving up and assuming the internal state machine
/// is stuck in an infinite loop
const MAX_INTERNAL_TRANSITIONS: usize = 20;

use internal_machines::InternalStateMachine;
use internal_machines::State as InternalState;

impl FirefoxAccount {
    /// Get the current state
    pub fn get_state(&self) -> FxaState {
        self.auth_state.clone()
    }

    /// Process an event (login, logout, etc).
    ///
    /// On success, returns the new state.
    /// On error, the state will remain the same.
    pub fn process_event(&mut self, event: FxaEvent) -> Result<FxaState> {
        match &self.auth_state {
            FxaState::Uninitialized => self.process_event_with_internal_state_machine(
                internal_machines::UninitializedStateMachine,
                event,
            ),
            FxaState::Disconnected => self.process_event_with_internal_state_machine(
                internal_machines::DisconnectedStateMachine,
                event,
            ),
            FxaState::Authenticating { .. } => self.process_event_with_internal_state_machine(
                internal_machines::AuthenticatingStateMachine,
                event,
            ),
            FxaState::Connected => self.process_event_with_internal_state_machine(
                internal_machines::ConnectedStateMachine,
                event,
            ),
            FxaState::AuthIssues => self.process_event_with_internal_state_machine(
                internal_machines::AuthIssuesStateMachine,
                event,
            ),
        }
    }

    fn process_event_with_internal_state_machine<T: InternalStateMachine>(
        &mut self,
        state_machine: T,
        event: FxaEvent,
    ) -> Result<FxaState> {
        let device_config = self.handle_state_machine_initialization(&event)?;

        breadcrumb!("FxaStateMachine.process_event starting: {event}");
        let mut internal_state = state_machine.initial_state(event)?;
        let mut count = 0;
        // Loop through internal state transitions until we reach a terminal state
        //
        // See `README.md` for details.
        loop {
            count += 1;
            if count > MAX_INTERNAL_TRANSITIONS {
                breadcrumb!("FxaStateMachine.process_event finished (MAX_INTERNAL_TRANSITIONS)");
                return Err(Error::StateMachineLogicError(
                    "infinite loop detected".to_owned(),
                ));
            }
            match internal_state {
                InternalState::Complete(new_state) => {
                    breadcrumb!("FxaStateMachine.process_event finished (Complete({new_state}))");
                    self.auth_state = new_state.clone();
                    return Ok(new_state);
                }
                InternalState::Cancel => {
                    breadcrumb!("FxaStateMachine.process_event finished (Cancel)");
                    return Ok(self.auth_state.clone());
                }
                state => {
                    let event = state.make_call(self, &device_config)?;
                    let event_msg = event.to_string();
                    internal_state = state_machine.next_state(state, event)?;
                    breadcrumb!("FxaStateMachine.process_event {event_msg} -> {internal_state}")
                }
            }
        }
    }

    /// Handles initialization before we process an event
    ///
    /// This checks that the first event we see is `FxaEvent::Initialize` and it returns the
    /// `DeviceConfig` from that event.
    fn handle_state_machine_initialization(&mut self, event: &FxaEvent) -> Result<DeviceConfig> {
        match &event {
            FxaEvent::Initialize { device_config } => match self.device_config {
                Some(_) => Err(Error::InvalidStateTransition(
                    "Initialize already sent".to_owned(),
                )),
                None => {
                    self.device_config = Some(device_config.clone());
                    Ok(device_config.clone())
                }
            },
            _ => match &self.device_config {
                Some(device_config) => Ok(device_config.clone()),
                None => Err(Error::InvalidStateTransition(
                    "Initialize not yet sent".to_owned(),
                )),
            },
        }
    }
}

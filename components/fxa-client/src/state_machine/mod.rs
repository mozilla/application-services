/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! FxA state machine
//!
//! This presents a high-level API for logging in, logging out, dealing with authentication token issues, etc.

use std::sync::Arc;

use error_support::{breadcrumb, convert_log_report_error, handle_error};
use parking_lot::Mutex;

use crate::{internal, ApiResult, DeviceConfig, Error, FirefoxAccount, Result};
mod checker;
mod logic;
mod types;

pub use checker::FxaStateMachineChecker;
pub use types::{Event, FxaEvent, FxaState, State};

use logic::next_state;

/// FxA state machine
///
/// This provides a high-level interface for using a [FirefoxAccount] -- login, logout, checking
/// auth status, etc.
pub struct FxaStateMachine {
    account: Arc<FirefoxAccount>,
    state: Mutex<FxaState>,
    oauth_entrypoint: String,
    device_config: DeviceConfig,
}

impl FxaStateMachine {
    /// Create an FxaStateMachine
    ///
    /// Note: When restoring a connected account, only `device_config.capabilities` will be used.
    /// We will use the device type and device name info that's stored on the server.
    pub fn new(
        account: Arc<FirefoxAccount>,
        oauth_entrypoint: String,
        device_config: DeviceConfig,
    ) -> Self {
        Self {
            account,
            state: Mutex::new(FxaState::Uninitialized),
            oauth_entrypoint,
            device_config,
        }
    }

    /// Get the current state
    pub fn state(&self) -> FxaState {
        self.state.lock().clone()
    }

    /// Process an event (login, logout, etc).
    ///
    /// On success, returns the new state.
    /// On error, the state will remain the same.
    #[handle_error(Error)]
    pub fn process_event(&self, event: FxaEvent) -> ApiResult<FxaState> {
        breadcrumb!("FxaStateMachine.process_event starting");
        let mut account = self.account.internal.lock();
        let mut current_state = self.state.lock();

        // Advance the state machine to an internal state
        let mut state = next_state(current_state.clone().into(), event.into(), &current_state)?;

        // Keep advancing the state machine until we reach a public state
        let mut count = 0;
        loop {
            match state.try_into_public_state() {
                Ok(public_state) => {
                    *current_state = public_state.clone();
                    breadcrumb!("FxaStateMachine.process_event finished");
                    return Ok(public_state);
                }
                Err(internal_state) => {
                    let event = match self.process_internal_state(&mut account, &internal_state) {
                        Ok(event) => event,
                        Err(e) => match e {
                            // We we passed a state to `process_internal_state` that it didn't
                            // expect, give up on processing the event.
                            Error::StateMachineLogicError(_) => return Err(e),
                            // For other errors, log/report them.
                            // Throw away the converted error -- the state machine just needs to
                            // process `Event::CallError` to get to the next state.
                            _ => {
                                convert_log_report_error(e);
                                Event::CallError
                            }
                        },
                    };
                    state = next_state(internal_state, event, &current_state)?;
                }
            };
            // Check that we're not just spinning our wheels and performing endless transitions
            count += 1;
            if count > 100 {
                breadcrumb!("FxaStateMachine.process_event finished");
                return Err(Error::StateMachineLogicError(
                    "infinite loop detected".to_owned(),
                ));
            }
        }
    }

    /// Perform the [FirefoxAccount] call for an internal state
    ///
    /// Returns the Event that's the result of the call
    pub fn process_internal_state(
        &self,
        account: &mut internal::FirefoxAccount,
        state: &State,
    ) -> Result<Event> {
        Ok(match state {
            State::GetAuthState => Event::GetAuthStateSuccess {
                auth_state: account.get_auth_state(),
            },
            State::BeginOAuthFlow { scopes } => {
                let scopes: Vec<&str> = scopes.iter().map(String::as_str).collect();
                let oauth_url = account.begin_oauth_flow(&scopes, &self.oauth_entrypoint)?;
                Event::BeginOAuthFlowSuccess { oauth_url }
            }
            State::BeginPairingFlow {
                pairing_url,
                scopes,
            } => {
                let scopes: Vec<&str> = scopes.iter().map(String::as_str).collect();
                let oauth_url =
                    account.begin_pairing_flow(pairing_url, &scopes, &self.oauth_entrypoint)?;
                Event::BeginOAuthFlowSuccess { oauth_url }
            }
            State::CompleteOAuthFlow { code, state } => {
                account.complete_oauth_flow(code, state)?;
                Event::CompleteOAuthFlowSuccess
            }
            State::InitializeDevice => {
                account.initialize_device(
                    &self.device_config.name,
                    self.device_config.device_type,
                    &self.device_config.capabilities,
                )?;
                Event::InitializeDeviceSuccess
            }
            State::EnsureDeviceCapabilities => {
                account.ensure_capabilities(&self.device_config.capabilities)?;
                Event::EnsureDeviceCapabilitiesSuccess
            }
            State::CheckAuthorizationStatus => {
                let status = account.check_authorization_status()?;
                Event::CheckAuthorizationStatusSuccess {
                    active: status.active,
                }
            }
            State::Disconnect => {
                account.disconnect();
                Event::DisconnectSuccess
            }
            _ => {
                return Err(Error::StateMachineLogicError(format!(
                    "invalid state in process_internal_state: {state}"
                )))
            }
        })
    }
}

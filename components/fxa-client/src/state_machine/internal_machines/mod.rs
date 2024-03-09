/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Internal state machine code

mod auth_issues;
mod authenticating;
mod connected;
mod disconnected;
mod uninitialized;

use crate::{
    internal::FirefoxAccount, DeviceConfig, Error, FxaError, FxaEvent, FxaRustAuthState, FxaState,
    Result,
};
pub use auth_issues::AuthIssuesStateMachine;
pub use authenticating::AuthenticatingStateMachine;
pub use connected::ConnectedStateMachine;
pub use disconnected::DisconnectedStateMachine;
use error_support::convert_log_report_error;
pub use uninitialized::UninitializedStateMachine;

pub trait InternalStateMachine {
    /// Initial state to start handling an public event
    fn initial_state(&self, event: FxaEvent) -> Result<State>;

    /// State transition from an internal event
    fn next_state(&self, state: State, event: Event) -> Result<State>;
}

/// Internal state machine states
///
/// Most variants either represent a [FirefoxAccount] method call.
/// `Complete` and `Cancel` are a terminal states which indicate the public state transition is complete.
/// Each internal state machine uses the same `State` enum, but they only actually transition to a subset of the variants.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum State {
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
    Disconnect,
    GetProfile,
    /// Complete the current [FxaState] transition by transitioning to a new state
    Complete(FxaState),
    /// Complete the current [FxaState] transition by remaining at the current state
    Cancel,
}

/// Internal state machine events
///
/// These represent the results of the method calls for each internal state.
/// Each internal state machine uses the same `Event` enum, but they only actually respond to a subset of the variants.
#[derive(Clone, Debug)]
pub enum Event {
    GetAuthStateSuccess {
        auth_state: FxaRustAuthState,
    },
    BeginOAuthFlowSuccess {
        oauth_url: String,
    },
    BeginPairingFlowSuccess {
        oauth_url: String,
    },
    CompleteOAuthFlowSuccess,
    InitializeDeviceSuccess,
    EnsureDeviceCapabilitiesSuccess,
    CheckAuthorizationStatusSuccess {
        active: bool,
    },
    DisconnectSuccess,
    GetProfileSuccess,
    CallError,
    /// Auth error for the `ensure_capabilities` call that we do on startup.
    /// This should likely go away when we do https://bugzilla.mozilla.org/show_bug.cgi?id=1868418
    EnsureCapabilitiesAuthError,
}

impl State {
    /// Perform the [FirefoxAccount] method call that corresponds to this state
    pub fn make_call(
        &self,
        account: &mut FirefoxAccount,
        device_config: &DeviceConfig,
    ) -> Result<Event> {
        let mut error_handling = CallErrorHandler::new(self);
        loop {
            return match self.make_call_inner(account, device_config) {
                Ok(event) => Ok(event),
                Err(e) => match error_handling.handle_error(e, account) {
                    CallResult::Retry => continue,
                    CallResult::Finished(event) => Ok(event),
                    CallResult::InternalError(err) => Err(err),
                },
            };
        }
    }

    fn make_call_inner(
        &self,
        account: &mut FirefoxAccount,
        device_config: &DeviceConfig,
    ) -> Result<Event> {
        Ok(match self {
            State::GetAuthState => Event::GetAuthStateSuccess {
                auth_state: account.get_auth_state(),
            },
            State::EnsureDeviceCapabilities => {
                account.ensure_capabilities(&device_config.capabilities)?;
                Event::EnsureDeviceCapabilitiesSuccess
            }
            State::BeginOAuthFlow { scopes, entrypoint } => {
                let scopes: Vec<&str> = scopes.iter().map(String::as_str).collect();
                let oauth_url = account.begin_oauth_flow(&scopes, entrypoint)?;
                Event::BeginOAuthFlowSuccess { oauth_url }
            }
            State::BeginPairingFlow {
                pairing_url,
                scopes,
                entrypoint,
            } => {
                let scopes: Vec<&str> = scopes.iter().map(String::as_str).collect();
                let oauth_url = account.begin_pairing_flow(pairing_url, &scopes, entrypoint)?;
                Event::BeginPairingFlowSuccess { oauth_url }
            }
            State::CompleteOAuthFlow { code, state } => {
                account.complete_oauth_flow(code, state)?;
                Event::CompleteOAuthFlowSuccess
            }
            State::InitializeDevice => {
                account.initialize_device(
                    &device_config.name,
                    device_config.device_type,
                    &device_config.capabilities,
                )?;
                Event::InitializeDeviceSuccess
            }
            State::CheckAuthorizationStatus => {
                let active = account.check_authorization_status()?.active;
                Event::CheckAuthorizationStatusSuccess { active }
            }
            State::Disconnect => {
                account.disconnect();
                Event::DisconnectSuccess
            }
            State::GetProfile => {
                account.get_profile(true)?;
                Event::GetProfileSuccess
            }
            state => {
                return Err(Error::StateMachineLogicError(format!(
                    "process_call: Don't know how to handle {state}"
                )))
            }
        })
    }
}

/// Number of times to retry fxa calls in the face of network errors
const NETWORK_RETRY_LIMIT: usize = 3;

struct CallErrorHandler<'a> {
    network_retries: usize,
    auth_retries: usize,
    state: &'a State,
}

impl<'a> CallErrorHandler<'a> {
    fn new(state: &'a State) -> Self {
        Self {
            network_retries: 0,
            auth_retries: 0,
            state,
        }
    }

    fn handle_error(&mut self, e: Error, account: &mut FirefoxAccount) -> CallResult {
        // If we see a StateMachineLogicError, return it immediately
        if matches!(e, Error::StateMachineLogicError(_)) {
            return CallResult::InternalError(e);
        }
        // Report the error and convert it to `FxaError` which makes it easier to handle.
        // For example, multiple `Error` variants map to `FxaError::Authentication`.
        crate::warn!("handling error: {e}");
        match convert_log_report_error(e) {
            FxaError::Network => {
                if self.network_retries < NETWORK_RETRY_LIMIT {
                    self.network_retries += 1;
                    CallResult::Retry
                } else {
                    CallResult::Finished(Event::CallError)
                }
            }
            FxaError::Authentication => {
                if self.auth_retries < 1 && !matches!(self.state, State::CheckAuthorizationStatus) {
                    // Operations can fail with authentication errors when we have stale access
                    // token in our cache.  To try to recover from this we should:
                    //
                    //   - Clear the access token
                    //   - Call `check_authorization_status`.  If successful we can retry the operation.
                    account.clear_access_token_cache();
                    match account.check_authorization_status() {
                        Ok(status) if status.active => {
                            self.auth_retries += 1;
                            CallResult::Retry
                        }
                        _ => CallResult::Finished(self.event_for_auth_error()),
                    }
                } else {
                    CallResult::Finished(self.event_for_auth_error())
                }
            }
            _ => CallResult::Finished(Event::CallError),
        }
    }

    fn event_for_auth_error(&self) -> Event {
        if matches!(self.state, State::EnsureDeviceCapabilities) {
            Event::EnsureCapabilitiesAuthError
        } else {
            Event::CallError
        }
    }
}

/// The result of a single call to the FxA client
enum CallResult {
    /// The call finished, either successfully or unsuccessfully, and we have a new [Event] to
    /// process.
    Finished(Event),
    /// We should to retry the call after an auth/network error.
    Retry,
    /// There was an internal error when trying to make the call and we should bail on the internal
    /// state transition.
    InternalError(Error),
}

fn invalid_transition(state: State, event: Event) -> Result<State> {
    Err(Error::InvalidStateTransition(format!("{state} -> {event}")))
}

#[cfg(test)]
struct StateMachineTester<T> {
    state_machine: T,
    state: State,
}

#[cfg(test)]
impl<T: InternalStateMachine> StateMachineTester<T> {
    fn new(state_machine: T, event: FxaEvent) -> Self {
        let initial_state = state_machine
            .initial_state(event)
            .expect("Error getting initial state");
        Self {
            state_machine,
            state: initial_state,
        }
    }

    /// Transition to a new state based on an event
    fn next_state(&mut self, event: Event) {
        self.state = self.peek_next_state(event);
    }

    /// peek_next_state what the next state would be without transitioning to it
    fn peek_next_state(&self, event: Event) -> State {
        self.state_machine
            .next_state(self.state.clone(), event.clone())
            .unwrap_or_else(|e| {
                panic!(
                    "Error getting next state: {e} state: {:?} event: {event:?}",
                    self.state
                )
            })
    }
}

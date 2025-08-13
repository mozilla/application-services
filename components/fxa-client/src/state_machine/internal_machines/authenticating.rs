/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{invalid_transition, Event, InternalStateMachine, State};
use crate::{Error, FxaEvent, FxaState, Result};

pub struct AuthenticatingStateMachine;

// Save some typing
use Event::*;
use State::*;

impl InternalStateMachine for AuthenticatingStateMachine {
    fn initial_state(&self, event: FxaEvent) -> Result<State> {
        match event {
            FxaEvent::CompleteOAuthFlow { code, state } => Ok(CompleteOAuthFlow {
                code: code.clone(),
                state: state.clone(),
            }),
            FxaEvent::CancelOAuthFlow => Ok(Complete(FxaState::Disconnected)),
            // These next 2 cases allow apps to begin a new oauth flow when we're already in the
            // middle of an existing one.
            FxaEvent::BeginOAuthFlow { scopes, entrypoint } => {
                Ok(State::BeginOAuthFlow { scopes, entrypoint })
            }
            FxaEvent::BeginPairingFlow {
                pairing_url,
                scopes,
                entrypoint,
            } => Ok(State::BeginPairingFlow {
                pairing_url,
                scopes,
                entrypoint,
            }),
            e => Err(Error::InvalidStateTransition(format!(
                "Authenticating -> {e}"
            ))),
        }
    }

    fn next_state(&self, state: State, event: Event) -> Result<State> {
        Ok(match (state, event) {
            (CompleteOAuthFlow { .. }, CompleteOAuthFlowSuccess) => InitializeDevice,
            (CompleteOAuthFlow { .. }, CallError) => Complete(FxaState::Disconnected),
            (InitializeDevice, InitializeDeviceSuccess) => Complete(FxaState::Connected),
            (InitializeDevice, CallError) => Complete(FxaState::Disconnected),
            (BeginOAuthFlow { .. }, BeginOAuthFlowSuccess { oauth_url }) => {
                Complete(FxaState::Authenticating { oauth_url })
            }
            (BeginPairingFlow { .. }, BeginPairingFlowSuccess { oauth_url }) => {
                Complete(FxaState::Authenticating { oauth_url })
            }
            (BeginOAuthFlow { .. }, CallError) => Complete(FxaState::Disconnected),
            (BeginPairingFlow { .. }, CallError) => Complete(FxaState::Disconnected),
            (state, event) => return invalid_transition(state, event),
        })
    }
}

#[cfg(test)]
mod test {
    use super::super::StateMachineTester;
    use super::*;

    #[test]
    fn test_complete_oauth_flow() {
        let mut tester = StateMachineTester::new(
            AuthenticatingStateMachine,
            FxaEvent::CompleteOAuthFlow {
                code: "test-code".to_owned(),
                state: "test-state".to_owned(),
            },
        );
        assert_eq!(
            tester.state,
            CompleteOAuthFlow {
                code: "test-code".to_owned(),
                state: "test-state".to_owned(),
            }
        );
        assert_eq!(
            tester.peek_next_state(CallError),
            Complete(FxaState::Disconnected)
        );

        tester.next_state(CompleteOAuthFlowSuccess);
        assert_eq!(tester.state, InitializeDevice);
        assert_eq!(
            tester.peek_next_state(CallError),
            Complete(FxaState::Disconnected)
        );
        assert_eq!(
            tester.peek_next_state(InitializeDeviceSuccess),
            Complete(FxaState::Connected)
        );
    }

    #[test]
    fn test_cancel_oauth_flow() {
        let tester = StateMachineTester::new(AuthenticatingStateMachine, FxaEvent::CancelOAuthFlow);
        assert_eq!(tester.state, Complete(FxaState::Disconnected));
    }

    /// Test what happens if we get the `BeginOAuthFlow` when we're already in the middle of
    /// authentication.
    ///
    /// In this case, we should start a new flow.  Note: the code to handle the `BeginOAuthFlow`
    /// internal state will cancel the previous flow.
    #[test]
    fn test_begin_oauth_flow() {
        let tester = StateMachineTester::new(
            AuthenticatingStateMachine,
            FxaEvent::BeginOAuthFlow {
                scopes: vec!["profile".to_owned()],
                entrypoint: "test-entrypoint".to_owned(),
            },
        );
        assert_eq!(
            tester.state,
            BeginOAuthFlow {
                scopes: vec!["profile".to_owned()],
                entrypoint: "test-entrypoint".to_owned(),
            }
        );
        assert_eq!(
            tester.peek_next_state(CallError),
            Complete(FxaState::Disconnected)
        );
        assert_eq!(
            tester.peek_next_state(BeginOAuthFlowSuccess {
                oauth_url: "http://example.com/oauth-start".to_owned(),
            }),
            Complete(FxaState::Authenticating {
                oauth_url: "http://example.com/oauth-start".to_owned(),
            })
        );
    }

    /// Same as `test_begin_oauth_flow`, but for a paring flow
    #[test]
    fn test_begin_pairing_flow() {
        let tester = StateMachineTester::new(
            AuthenticatingStateMachine,
            FxaEvent::BeginPairingFlow {
                pairing_url: "https://example.com/pairing-url".to_owned(),
                scopes: vec!["profile".to_owned()],
                entrypoint: "test-entrypoint".to_owned(),
            },
        );
        assert_eq!(
            tester.state,
            BeginPairingFlow {
                pairing_url: "https://example.com/pairing-url".to_owned(),
                scopes: vec!["profile".to_owned()],
                entrypoint: "test-entrypoint".to_owned(),
            }
        );
        assert_eq!(
            tester.peek_next_state(CallError),
            Complete(FxaState::Disconnected)
        );
        assert_eq!(
            tester.peek_next_state(BeginPairingFlowSuccess {
                oauth_url: "http://example.com/oauth-start".to_owned(),
            }),
            Complete(FxaState::Authenticating {
                oauth_url: "http://example.com/oauth-start".to_owned(),
            })
        );
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::types::{Event, FxaState, State};
use crate::{Error, FxaRustAuthState, Result};
use error_support::report_error;

/// Advance the internal state machine
///
/// * For public states, if a valid event is sent then we transition to one of the internal states.
/// * The internal states correspond to [crate::internal::FirefoxAccount] call.
/// * Internal states are be transitioned out of with a event that corresponds to a successful
///   result, or `Event::CallError` if there was an error making the call.
///
/// prev_public_state is the state that the state machine was in before `process_event` was called.
pub fn next_state(state: State, event: Event, prev_public_state: &FxaState) -> Result<State> {
    Ok(match (state, event) {
        // Initialization transitions
        (State::Uninitialized, Event::Initialize) => State::GetAuthState,
        (State::GetAuthState, Event::GetAuthStateSuccess { auth_state }) => match auth_state {
            FxaRustAuthState::Disconnected => State::Disconnected,
            FxaRustAuthState::AuthIssues => State::AuthIssues,
            FxaRustAuthState::Connected => State::EnsureDeviceCapabilities,
        },
        (State::EnsureDeviceCapabilities, Event::EnsureDeviceCapabilitiesSuccess) => {
            State::Connected
        }
        (State::EnsureDeviceCapabilities, Event::CallError) => State::Disconnected,

        // Begin oauth flow transitions
        (State::Disconnected, Event::BeginOAuthFlow { scopes }) => State::BeginOAuthFlow { scopes },
        (State::BeginOAuthFlow { .. }, Event::BeginOAuthFlowSuccess { oauth_url }) => {
            State::Authenticating { oauth_url }
        }
        (State::BeginOAuthFlow { .. }, Event::CallError) => prev_public_state.clone().into(),

        // Begin pairing flow transitions
        (
            State::Disconnected,
            Event::BeginPairingFlow {
                pairing_url,
                scopes,
            },
        ) => State::BeginPairingFlow {
            pairing_url,
            scopes,
        },
        (State::BeginPairingFlow { .. }, Event::BeginPairingFlowSuccess { oauth_url }) => {
            State::Authenticating { oauth_url }
        }
        (State::BeginPairingFlow { .. }, Event::CallError) => State::Disconnected,

        // Complete oauth flow transitions
        (State::Authenticating { .. }, Event::CompleteOAuthFlow { code, state }) => {
            State::CompleteOAuthFlow { code, state }
        }
        (State::Authenticating { .. }, Event::CancelOAuthFlow) => State::Disconnected,
        (State::CompleteOAuthFlow { .. }, Event::CompleteOAuthFlowSuccess) => {
            State::InitializeDevice
        }
        (State::InitializeDevice, Event::EnsureDeviceCapabilitiesSuccess) => State::Connected,
        (State::CompleteOAuthFlow { .. }, Event::CallError) => prev_public_state.clone().into(),
        (State::InitializeDevice, Event::CallError) => prev_public_state.clone().into(),

        // Disconnect transitions
        (State::Connected, Event::Disconnect) => State::Disconnect,
        (State::Disconnect, Event::DisconnectSuccess) => State::Disconnected,
        (State::Disconnect, Event::CallError) => {
            // disconnect() is currently infallible, but let's handle errors anyway in case we
            // refactor it in the future.
            report_error!("fxa-state-machine-error", "saw CallError after Disconnect");
            State::Disconnected
        }

        // Check authorization status transitions
        (State::Connected, Event::CheckAuthorizationStatus) => State::CheckAuthorizationStatus,
        (State::CheckAuthorizationStatus, Event::CheckAuthorizationStatusSuccess { active }) => {
            if active {
                State::Connected
            } else {
                State::Disconnected
            }
        }
        (State::CheckAuthorizationStatus, Event::CallError) => State::Disconnected,

        // Reauthorization from AuthIssues
        (State::AuthIssues, Event::BeginOAuthFlow { scopes }) => State::BeginOAuthFlow { scopes },

        // All other transitions are errors
        (state, event) => {
            return Err(Error::InvalidStateTransition(format!(
                "({state} -> {event})"
            )))
        }
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Clone)]
    struct StateMachineTester {
        state: State,
        initial_public_state: FxaState,
    }

    impl StateMachineTester {
        fn new(initial_public_state: FxaState) -> Self {
            Self {
                state: initial_public_state.clone().into(),
                initial_public_state,
            }
        }

        // Calculate the next state, but don't move to it.  Useful for testing error branches.
        fn peek_next_state(&self, event: Event) -> State {
            next_state(self.state.clone(), event, &self.initial_public_state).unwrap()
        }

        fn advance_to_next_state(&mut self, event: Event) -> State {
            let new_state = self.peek_next_state(event);
            self.state = new_state.clone();
            new_state
        }

        fn is_event_invalid(&self, event: Event) -> bool {
            next_state(self.state.clone(), event, &self.initial_public_state).is_err()
        }
    }

    #[test]
    fn test_initialize() {
        let mut tester = StateMachineTester::new(FxaState::Uninitialized);
        assert_eq!(
            tester.advance_to_next_state(Event::Initialize),
            State::GetAuthState
        );
        assert_eq!(
            tester.peek_next_state(Event::GetAuthStateSuccess {
                auth_state: FxaRustAuthState::Disconnected
            }),
            State::Disconnected
        );
        assert_eq!(
            tester.peek_next_state(Event::GetAuthStateSuccess {
                auth_state: FxaRustAuthState::AuthIssues
            }),
            State::AuthIssues
        );

        assert_eq!(
            tester.advance_to_next_state(Event::GetAuthStateSuccess {
                auth_state: FxaRustAuthState::Connected
            }),
            State::EnsureDeviceCapabilities
        );
        assert_eq!(
            tester.peek_next_state(Event::CallError),
            State::Disconnected
        );
        assert_eq!(
            tester.advance_to_next_state(Event::EnsureDeviceCapabilitiesSuccess),
            State::Connected
        );
    }

    #[test]
    fn test_oauth_flow() {
        let mut tester = StateMachineTester::new(FxaState::Disconnected);
        assert_eq!(
            tester.advance_to_next_state(Event::BeginOAuthFlow {
                scopes: vec!["profile".to_owned()],
            }),
            State::BeginOAuthFlow {
                scopes: vec!["profile".to_owned()],
            }
        );
        assert_eq!(
            tester.peek_next_state(Event::CallError),
            State::Disconnected
        );
        assert_eq!(
            tester.advance_to_next_state(Event::BeginOAuthFlowSuccess {
                oauth_url: "http://example.com/oauth-start".to_owned()
            }),
            State::Authenticating {
                oauth_url: "http://example.com/oauth-start".to_owned(),
            }
        );
    }

    #[test]
    fn test_pairing_flow() {
        let mut tester = StateMachineTester::new(FxaState::Disconnected);
        assert_eq!(
            tester.advance_to_next_state(Event::BeginPairingFlow {
                pairing_url: "https://example.com/pairing-url".to_owned(),
                scopes: vec!["profile".to_owned()],
            },),
            State::BeginPairingFlow {
                pairing_url: "https://example.com/pairing-url".to_owned(),
                scopes: vec!["profile".to_owned()],
            }
        );
        assert_eq!(
            tester.peek_next_state(Event::CallError),
            State::Disconnected,
        );
        assert_eq!(
            tester.advance_to_next_state(Event::BeginPairingFlowSuccess {
                oauth_url: "http://example.com/oauth-start".to_owned()
            },),
            State::Authenticating {
                oauth_url: "http://example.com/oauth-start".to_owned(),
            }
        );
    }

    #[test]
    fn test_complete_oauth_flow() {
        let mut tester = StateMachineTester::new(FxaState::Authenticating {
            oauth_url: "http://example.com/oauth-start".to_owned(),
        });
        assert_eq!(
            tester.peek_next_state(Event::CancelOAuthFlow),
            State::Disconnected,
        );
        assert_eq!(
            tester.advance_to_next_state(Event::CompleteOAuthFlow {
                code: "test-code".to_owned(),
                state: "test-state".to_owned(),
            }),
            State::CompleteOAuthFlow {
                code: "test-code".to_owned(),
                state: "test-state".to_owned(),
            },
        );
        assert_eq!(
            tester.peek_next_state(Event::CallError),
            State::Authenticating {
                oauth_url: "http://example.com/oauth-start".to_owned(),
            },
        );
        assert_eq!(
            tester.advance_to_next_state(Event::CompleteOAuthFlowSuccess),
            State::InitializeDevice,
        );
        assert_eq!(
            tester.peek_next_state(Event::CallError),
            State::Authenticating {
                oauth_url: "http://example.com/oauth-start".to_owned(),
            },
        );
        assert_eq!(
            tester.advance_to_next_state(Event::EnsureDeviceCapabilitiesSuccess),
            State::Connected,
        );
    }

    #[test]
    fn test_disconnect() {
        let mut tester = StateMachineTester::new(FxaState::Connected);
        assert_eq!(
            tester.advance_to_next_state(Event::Disconnect),
            State::Disconnect
        );
        assert_eq!(
            tester.peek_next_state(Event::DisconnectSuccess),
            State::Disconnected
        );
        assert_eq!(
            tester.peek_next_state(Event::CallError),
            State::Disconnected
        );
    }

    #[test]
    fn test_check_authorization() {
        let mut tester = StateMachineTester::new(FxaState::Connected);
        assert_eq!(
            tester.advance_to_next_state(Event::CheckAuthorizationStatus),
            State::CheckAuthorizationStatus
        );
        assert_eq!(
            tester.peek_next_state(Event::CheckAuthorizationStatusSuccess { active: true }),
            State::Connected
        );
        assert_eq!(
            tester.peek_next_state(Event::CheckAuthorizationStatusSuccess { active: false }),
            State::Disconnected
        );
        assert_eq!(
            tester.peek_next_state(Event::CallError),
            State::Disconnected
        );
    }

    #[test]
    fn test_reauthenticate() {
        let mut tester = StateMachineTester::new(FxaState::AuthIssues);
        // Pairing flow is intended for connecting new devices only and isn't valid for
        // reauthentication after auth issues.
        assert!(tester.is_event_invalid(Event::BeginPairingFlow {
            pairing_url: "https://example.com/pairing-url".to_owned(),
            scopes: vec!["profile".to_owned()],
        }));
        assert_eq!(
            tester.advance_to_next_state(Event::BeginOAuthFlow {
                scopes: vec!["profile".to_owned()],
            }),
            State::BeginOAuthFlow {
                scopes: vec!["profile".to_owned()],
            }
        );
        assert_eq!(tester.peek_next_state(Event::CallError), State::AuthIssues);
        assert_eq!(
            tester.advance_to_next_state(Event::BeginOAuthFlowSuccess {
                oauth_url: "http://example.com/oauth-start".to_owned()
            }),
            State::Authenticating {
                oauth_url: "http://example.com/oauth-start".to_owned(),
            }
        );
    }
}

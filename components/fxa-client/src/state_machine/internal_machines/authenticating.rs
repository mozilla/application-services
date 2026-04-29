/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{invalid_transition, Event, InternalStateMachine, State};
use crate::{Error, FxaEvent, FxaRustAuthState, FxaState, Result};
use error_support::report_error;

pub struct AuthenticatingStateMachine {
    pub initial_state: FxaRustAuthState,
}

// Save some typing
use Event::*;
use State::*;

impl InternalStateMachine for AuthenticatingStateMachine {
    fn initial_state(&self, event: FxaEvent) -> Result<State> {
        match event {
            FxaEvent::CompleteOAuthFlow { code, state } => Ok(CompleteOAuthFlow {
                code: code.clone(),
                state: state.clone(),
                initial_state: self.initial_state,
            }),
            FxaEvent::CancelOAuthFlow => Ok(Complete(self.initial_state.into())),
            FxaEvent::Disconnect => Ok(Disconnect),
            // These next 2 cases allow apps to begin a new oauth flow when we're already in the
            // middle of an existing one. (Supporting new flows in this state makes sense, but
            // allowing multiple active flows, which we kinda do, probably doesn't really work?)
            FxaEvent::BeginOAuthFlow {
                service,
                scopes,
                entrypoint,
            } => Ok(State::BeginOAuthFlow {
                service,
                scopes,
                entrypoint,
                initial_state: self.initial_state,
            }),
            FxaEvent::BeginPairingFlow {
                service,
                pairing_url,
                scopes,
                entrypoint,
            } => Ok(State::BeginPairingFlow {
                service,
                pairing_url,
                scopes,
                entrypoint,
                initial_state: self.initial_state,
            }),
            e => Err(Error::InvalidStateTransition(format!(
                "Authenticating -> {e}"
            ))),
        }
    }

    fn next_state(&self, state: State, event: Event) -> Result<State> {
        Ok(match (state, event) {
            (
                CompleteOAuthFlow {
                    initial_state: FxaRustAuthState::Connected,
                    ..
                },
                CompleteOAuthFlowSuccess,
            ) => Complete(FxaState::Connected),
            (CompleteOAuthFlow { .. }, CompleteOAuthFlowSuccess) => InitializeDevice,
            (CompleteOAuthFlow { .. }, CallError) => Complete(self.initial_state.into()),
            (Disconnect, DisconnectSuccess) => Complete(FxaState::Disconnected),
            (Disconnect, CallError) => {
                // disconnect() is currently infallible, but let's handle errors anyway in case we
                // refactor it in the future.
                report_error!("fxa-state-machine-error", "saw CallError after Disconnect");
                Complete(FxaState::Disconnected)
            }
            (InitializeDevice, InitializeDeviceSuccess) => Complete(FxaState::Connected),
            (InitializeDevice, CallError) => Complete(FxaState::Disconnected),
            (BeginOAuthFlow { initial_state, .. }, BeginOAuthFlowSuccess { oauth_url }) => {
                Complete(FxaState::Authenticating {
                    oauth_url,
                    initial_state,
                })
            }
            (BeginPairingFlow { initial_state, .. }, BeginPairingFlowSuccess { oauth_url }) => {
                Complete(FxaState::Authenticating {
                    oauth_url,
                    initial_state,
                })
            }
            (BeginOAuthFlow { .. }, CallError) => Complete(self.initial_state.into()),
            (BeginPairingFlow { .. }, CallError) => Complete(self.initial_state.into()),
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
            AuthenticatingStateMachine {
                initial_state: FxaRustAuthState::Disconnected,
            },
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
                initial_state: FxaRustAuthState::Disconnected,
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
    fn test_complete_oauth_flow_connected() {
        let mut tester = StateMachineTester::new(
            AuthenticatingStateMachine {
                initial_state: FxaRustAuthState::Connected,
            },
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
                initial_state: FxaRustAuthState::Connected,
            }
        );
        assert_eq!(
            tester.peek_next_state(CallError),
            Complete(FxaState::Connected)
        );

        tester.next_state(CompleteOAuthFlowSuccess);
        assert_eq!(tester.state, Complete(FxaState::Connected));
    }

    #[test]
    fn test_cancel_oauth_flow() {
        let tester = StateMachineTester::new(
            AuthenticatingStateMachine {
                initial_state: FxaRustAuthState::Connected,
            },
            FxaEvent::CancelOAuthFlow,
        );
        assert_eq!(tester.state, Complete(FxaState::Connected));

        let tester = StateMachineTester::new(
            AuthenticatingStateMachine {
                initial_state: FxaRustAuthState::Disconnected,
            },
            FxaEvent::CancelOAuthFlow,
        );
        assert_eq!(tester.state, Complete(FxaState::Disconnected));

        let tester = StateMachineTester::new(
            AuthenticatingStateMachine {
                initial_state: FxaRustAuthState::AuthIssues,
            },
            FxaEvent::CancelOAuthFlow,
        );
        assert_eq!(tester.state, Complete(FxaState::AuthIssues));
    }

    /// Test what happens if we get the `BeginOAuthFlow` when we're already in the middle of
    /// authentication.
    ///
    /// In this case, we should start a new flow.  Note: the code to handle the `BeginOAuthFlow`
    /// internal state will cancel the previous flow.
    #[test]
    fn test_begin_oauth_flow() {
        let tester = StateMachineTester::new(
            AuthenticatingStateMachine {
                initial_state: FxaRustAuthState::Disconnected,
            },
            FxaEvent::BeginOAuthFlow {
                service: "service".to_owned(),
                scopes: vec!["profile".to_owned()],
                entrypoint: "test-entrypoint".to_owned(),
            },
        );
        assert_eq!(
            tester.state,
            BeginOAuthFlow {
                service: "service".to_owned(),
                scopes: vec!["profile".to_owned()],
                entrypoint: "test-entrypoint".to_owned(),
                initial_state: FxaRustAuthState::Disconnected,
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
                initial_state: FxaRustAuthState::Disconnected,
            })
        );
    }

    /// Same as `test_begin_oauth_flow`, but for a paring flow
    #[test]
    fn test_begin_pairing_flow() {
        let tester = StateMachineTester::new(
            AuthenticatingStateMachine {
                initial_state: FxaRustAuthState::Disconnected,
            },
            FxaEvent::BeginPairingFlow {
                service: "service".to_owned(),
                pairing_url: "https://example.com/pairing-url".to_owned(),
                scopes: vec!["profile".to_owned()],
                entrypoint: "test-entrypoint".to_owned(),
            },
        );
        assert_eq!(
            tester.state,
            BeginPairingFlow {
                service: "service".to_owned(),
                pairing_url: "https://example.com/pairing-url".to_owned(),
                scopes: vec!["profile".to_owned()],
                entrypoint: "test-entrypoint".to_owned(),
                initial_state: FxaRustAuthState::Disconnected,
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
                initial_state: FxaRustAuthState::Disconnected,
            })
        );
    }

    #[test]
    fn test_disconnect_during_oauth_flow() {
        let tester = StateMachineTester::new(
            AuthenticatingStateMachine {
                initial_state: FxaRustAuthState::Disconnected,
            },
            FxaEvent::Disconnect,
        );
        assert_eq!(
            tester.peek_next_state(CallError),
            Complete(FxaState::Disconnected)
        );
        assert_eq!(
            tester.peek_next_state(DisconnectSuccess),
            Complete(FxaState::Disconnected)
        );
    }
}

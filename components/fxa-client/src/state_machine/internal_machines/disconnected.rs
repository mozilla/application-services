/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{invalid_transition, Event, InternalStateMachine, State};
use crate::{Error, FxaEvent, FxaState, Result};

pub struct DisconnectedStateMachine;

// Save some typing
use Event::*;
use State::*;

impl InternalStateMachine for DisconnectedStateMachine {
    fn initial_state(&self, event: FxaEvent) -> Result<State> {
        match event {
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
                "Disconnected -> {e}"
            ))),
        }
    }

    fn next_state(&self, state: State, event: Event) -> Result<State> {
        Ok(match (state, event) {
            (BeginOAuthFlow { .. }, BeginOAuthFlowSuccess { oauth_url }) => {
                Complete(FxaState::Authenticating { oauth_url })
            }
            (BeginPairingFlow { .. }, BeginPairingFlowSuccess { oauth_url }) => {
                Complete(FxaState::Authenticating { oauth_url })
            }
            (BeginOAuthFlow { .. }, CallError) => Cancel,
            (BeginPairingFlow { .. }, CallError) => Cancel,
            (state, event) => return invalid_transition(state, event),
        })
    }
}

#[cfg(test)]
mod test {
    use super::super::StateMachineTester;
    use super::*;

    #[test]
    fn test_oauth_flow() {
        let tester = StateMachineTester::new(
            DisconnectedStateMachine,
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
        assert_eq!(tester.peek_next_state(CallError), Cancel);
        assert_eq!(
            tester.peek_next_state(BeginOAuthFlowSuccess {
                oauth_url: "http://example.com/oauth-start".to_owned(),
            }),
            Complete(FxaState::Authenticating {
                oauth_url: "http://example.com/oauth-start".to_owned(),
            })
        );
    }

    #[test]
    fn test_pairing_flow() {
        let tester = StateMachineTester::new(
            DisconnectedStateMachine,
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
        assert_eq!(tester.peek_next_state(CallError), Cancel);
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

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{invalid_transition, Event, InternalStateMachine, State};
use crate::{Error, FxaEvent, FxaState, Result};

pub struct AuthIssuesStateMachine;

// Save some typing
use Event::*;
use State::*;

impl InternalStateMachine for AuthIssuesStateMachine {
    fn initial_state(&self, event: FxaEvent) -> Result<State> {
        match event {
            FxaEvent::BeginOAuthFlow { scopes, entrypoint } => Ok(BeginOAuthFlow {
                scopes: scopes.clone(),
                entrypoint: entrypoint.clone(),
            }),
            FxaEvent::Disconnect => Ok(Complete(FxaState::Disconnected)),
            e => Err(Error::InvalidStateTransition(format!("AuthIssues -> {e}"))),
        }
    }

    fn next_state(&self, state: State, event: Event) -> Result<State> {
        Ok(match (state, event) {
            (BeginOAuthFlow { .. }, BeginOAuthFlowSuccess { oauth_url }) => {
                Complete(FxaState::Authenticating { oauth_url })
            }
            (BeginOAuthFlow { .. }, CallError) => Cancel,
            (state, event) => return invalid_transition(state, event),
        })
    }
}

#[cfg(test)]
mod test {
    use super::super::StateMachineTester;
    use super::*;

    #[test]
    fn test_reauthenticate() {
        let tester = StateMachineTester::new(
            AuthIssuesStateMachine,
            FxaEvent::BeginOAuthFlow {
                scopes: vec!["profile".to_owned()],
                entrypoint: "test-entrypoint".to_owned(),
            },
        );

        assert_eq!(
            tester.state,
            BeginOAuthFlow {
                scopes: vec!["profile".to_owned()],
                entrypoint: "test-entrypoint".to_owned()
            }
        );
        assert_eq!(tester.peek_next_state(CallError), Cancel);
        assert_eq!(
            tester.peek_next_state(BeginOAuthFlowSuccess {
                oauth_url: "http://example.com/oauth-start".to_owned()
            }),
            Complete(FxaState::Authenticating {
                oauth_url: "http://example.com/oauth-start".to_owned(),
            })
        );
    }

    #[test]
    fn test_disconnect() {
        let tester = StateMachineTester::new(AuthIssuesStateMachine, FxaEvent::Disconnect);
        assert_eq!(tester.state, Complete(FxaState::Disconnected));
    }
}

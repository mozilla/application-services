/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{invalid_transition, Event, InternalStateMachine, State};
use crate::{Error, FxaEvent, FxaState, Result};
use error_support::report_error;

pub struct ConnectedStateMachine;

// Save some typing
use Event::*;
use State::*;

impl InternalStateMachine for ConnectedStateMachine {
    fn initial_state(&self, event: FxaEvent) -> Result<State> {
        match event {
            FxaEvent::Disconnect => Ok(Disconnect),
            FxaEvent::CheckAuthorizationStatus => Ok(CheckAuthorizationStatus),
            FxaEvent::CallGetProfile => Ok(GetProfile),
            e => Err(Error::InvalidStateTransition(format!("Connected -> {e}"))),
        }
    }

    fn next_state(&self, state: State, event: Event) -> Result<State> {
        Ok(match (state, event) {
            (Disconnect, DisconnectSuccess) => Complete(FxaState::Disconnected),
            (Disconnect, CallError) => {
                // disconnect() is currently infallible, but let's handle errors anyway in case we
                // refactor it in the future.
                report_error!("fxa-state-machine-error", "saw CallError after Disconnect");
                Complete(FxaState::Disconnected)
            }
            (CheckAuthorizationStatus, CheckAuthorizationStatusSuccess { active }) => {
                if active {
                    Complete(FxaState::Connected)
                } else {
                    Complete(FxaState::AuthIssues)
                }
            }
            (GetProfile, GetProfileSuccess) => Complete(FxaState::Connected),
            (GetProfile, CallError) => Complete(FxaState::AuthIssues),
            (CheckAuthorizationStatus, CallError) => Complete(FxaState::AuthIssues),
            (state, event) => return invalid_transition(state, event),
        })
    }
}

#[cfg(test)]
mod test {
    use super::super::StateMachineTester;
    use super::*;

    #[test]
    fn test_disconnect() {
        let tester = StateMachineTester::new(ConnectedStateMachine, FxaEvent::Disconnect);
        assert_eq!(tester.state, Disconnect);
        assert_eq!(
            tester.peek_next_state(CallError),
            Complete(FxaState::Disconnected)
        );
        assert_eq!(
            tester.peek_next_state(DisconnectSuccess),
            Complete(FxaState::Disconnected)
        );
    }

    #[test]
    fn test_check_authorization() {
        let tester =
            StateMachineTester::new(ConnectedStateMachine, FxaEvent::CheckAuthorizationStatus);
        assert_eq!(tester.state, CheckAuthorizationStatus);
        assert_eq!(
            tester.peek_next_state(CallError),
            Complete(FxaState::AuthIssues)
        );
        assert_eq!(
            tester.peek_next_state(CheckAuthorizationStatusSuccess { active: true }),
            Complete(FxaState::Connected),
        );
        assert_eq!(
            tester.peek_next_state(CheckAuthorizationStatusSuccess { active: false }),
            Complete(FxaState::AuthIssues)
        );
    }
}

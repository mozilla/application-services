/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{invalid_transition, Event, InternalStateMachine, State};
use crate::{Error, FxaEvent, FxaRustAuthState, FxaState, Result};

pub struct UninitializedStateMachine;

// Save some typing
use Event::*;
use State::*;

impl InternalStateMachine for UninitializedStateMachine {
    fn initial_state(&self, event: FxaEvent) -> Result<State> {
        match event {
            FxaEvent::Initialize { .. } => Ok(GetAuthState),
            e => Err(Error::InvalidStateTransition(format!(
                "Uninitialized -> {e}"
            ))),
        }
    }

    fn next_state(&self, state: State, event: Event) -> Result<State> {
        Ok(match (state, event) {
            (GetAuthState, GetAuthStateSuccess { auth_state }) => match auth_state {
                FxaRustAuthState::Disconnected => Complete(FxaState::Disconnected),
                FxaRustAuthState::AuthIssues => {
                    // FIXME: We should move to `AuthIssues` here, but we don't in order to
                    // match the current firefox-android behavior
                    // See https://bugzilla.mozilla.org/show_bug.cgi?id=1794212
                    EnsureDeviceCapabilities
                }
                FxaRustAuthState::Connected => EnsureDeviceCapabilities,
            },
            (EnsureDeviceCapabilities, EnsureDeviceCapabilitiesSuccess) => {
                Complete(FxaState::Connected)
            }
            (EnsureDeviceCapabilities, CallError) => Complete(FxaState::Disconnected),
            (EnsureDeviceCapabilities, EnsureCapabilitiesAuthError) => CheckAuthorizationStatus,

            // FIXME: we should re-run `ensure_capabilities` in this case, but we don't in order to
            // match the current firefox-android behavior.
            // See https://bugzilla.mozilla.org/show_bug.cgi?id=1868418
            (CheckAuthorizationStatus, CheckAuthorizationStatusSuccess { active: true }) => {
                Complete(FxaState::Connected)
            }
            (CheckAuthorizationStatus, CheckAuthorizationStatusSuccess { active: false })
            | (CheckAuthorizationStatus, CallError) => Complete(FxaState::AuthIssues),
            (state, event) => return invalid_transition(state, event),
        })
    }
}

#[cfg(test)]
mod test {
    use super::super::StateMachineTester;
    use super::*;
    use crate::{DeviceConfig, DeviceType};

    #[test]
    fn test_state_machine() {
        let mut tester = StateMachineTester::new(
            UninitializedStateMachine,
            FxaEvent::Initialize {
                device_config: DeviceConfig {
                    name: "test-device".to_owned(),
                    device_type: DeviceType::Mobile,
                    capabilities: vec![],
                },
            },
        );
        assert_eq!(tester.state, GetAuthState);
        assert_eq!(
            tester.peek_next_state(GetAuthStateSuccess {
                auth_state: FxaRustAuthState::Disconnected
            }),
            Complete(FxaState::Disconnected)
        );
        assert_eq!(
            tester.peek_next_state(GetAuthStateSuccess {
                auth_state: FxaRustAuthState::AuthIssues
            }),
            // FIXME: https://bugzilla.mozilla.org/show_bug.cgi?id=1794212
            EnsureDeviceCapabilities,
        );

        tester.next_state(GetAuthStateSuccess {
            auth_state: FxaRustAuthState::Connected,
        });
        assert_eq!(tester.state, EnsureDeviceCapabilities);
        assert_eq!(
            tester.peek_next_state(CallError),
            Complete(FxaState::Disconnected)
        );
        assert_eq!(
            tester.peek_next_state(EnsureDeviceCapabilitiesSuccess),
            Complete(FxaState::Connected)
        );

        tester.next_state(EnsureCapabilitiesAuthError);
        assert_eq!(tester.state, CheckAuthorizationStatus);
        assert_eq!(
            tester.peek_next_state(CallError),
            Complete(FxaState::AuthIssues)
        );
        assert_eq!(
            tester.peek_next_state(CheckAuthorizationStatusSuccess { active: false }),
            Complete(FxaState::AuthIssues)
        );
        assert_eq!(
            tester.peek_next_state(CheckAuthorizationStatusSuccess { active: true }),
            Complete(FxaState::Connected)
        );
    }
}

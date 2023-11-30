/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fmt;

use crate::FxaRustAuthState;

/// Fxa state
///
/// These are the states of [crate::FxaStateMachine] that consumers observe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FxaState {
    /// The state machine needs to be initialized via [Event::Initialize].
    Uninitialized,
    /// User has not connected to FxA or has logged out
    Disconnected,
    /// User is currently performing an OAuth flow
    Authenticating { oauth_url: String },
    /// User is currently connected to FxA
    Connected,
    /// User was connected to FxA, but we observed issues with the auth tokens.
    /// The user needs to reauthenticate before the account can be used.
    AuthIssues,
}

/// Fxa event
///
/// These are the events that consumers send to [crate::FxaStateMachine::process_event]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FxaEvent {
    /// Initialize the state machine.  This must be the first event sent.
    Initialize,
    /// Begin an oauth flow
    ///
    /// If successful, the state machine will transition the [FxaState::Authenticating].  The next
    /// step is to navigate the user to the `oauth_url` and let them sign and authorize the client.
    BeginOAuthFlow { scopes: Vec<String> },
    /// Begin an oauth flow using a URL from a pairing code
    ///
    /// If successful, the state machine will transition the [FxaState::Authenticating].  The next
    /// step is to navigate the user to the `oauth_url` and let them sign and authorize the client.
    BeginPairingFlow {
        pairing_url: String,
        scopes: Vec<String>,
    },
    /// Complete an OAuth flow.
    ///
    /// Send this event after the user has navigated through the OAuth flow and has reached the
    /// redirect URI.  Extract `code` and `state` from the query parameters.  If successful the
    /// state machine will transition to [FxaState::Connected].
    CompleteOAuthFlow { code: String, state: String },
    /// Cancel an OAuth flow.
    ///
    /// Use this to cancel an in-progress OAuth, returning to [FxaState::Disconnected] so the
    /// process can begin again.
    CancelOAuthFlow,
    /// Check the authorization status for a connected account.
    ///
    /// Send this when issues are detected with the auth tokens for a connected account.  It will
    /// double check for authentication issues with the account.  If it detects them, the state
    /// machine will transition to [FxaState::AuthIssues].  From there you can start an OAuth flow
    /// again to re-connect the user.
    CheckAuthorizationStatus,
    /// Disconnect the user
    ///
    /// Send this when the user is asking to be logged out.  The state machine will transition to
    /// [FxaState::Disconnected].
    Disconnect,
}

/// Internal [crate::FxaStateMachine] states
///
/// For each [FxaState] variant, there is a corresponding variant here.  These are called the
/// public states since they are visible to consumer applications.
///
/// There are also variants for internal states.  This states indicate that the state machine is
/// making a [crate::internal::FirefoxAccount] call. Internal states are temporary.
/// [crate::FxaStateMachine] will transition to a new state once the call is complete and
/// before `process_event` completes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    // Public states
    Uninitialized,
    Disconnected,
    Authenticating {
        oauth_url: String,
    },
    Connected,
    AuthIssues,
    // Internal states
    //
    // Note: right now things are extremely simple because each `process_event()` transition makes
    // a distinct set of FirefoxAccount calls, but this might not be true in the future. For
    // example, if we defined a `finalize_device` method that merged the work of
    // `initialize_device` and `ensure_capabilities`, then we would want to call that when handling
    // both the `Initialize` and `CompleteOAuthFlow` events.  In that case, we might want to define
    // both a `FinalizeDeviceForInitialize` and `FinalizeDeviceForLogin` state.
    GetAuthState,
    BeginOAuthFlow {
        scopes: Vec<String>,
    },
    BeginPairingFlow {
        pairing_url: String,
        scopes: Vec<String>,
    },
    CompleteOAuthFlow {
        code: String,
        state: String,
    },
    InitializeDevice,
    EnsureDeviceCapabilities,
    CheckAuthorizationStatus,
    Disconnect,
}

impl State {
    pub fn try_into_public_state(self) -> Result<FxaState, State> {
        self.try_into()
    }
}

/// Internal [crate::FxaStateMachine] events
///
/// For each [FxaEvent] variant, there is a corresponding variant here.  These are called the
/// public events, since they are visible to consumer applications.
///
/// There are also variants for interval events that represent the result of a
/// [crate::internal::FirefoxAccount] call.
#[derive(Clone, Debug)]
pub enum Event {
    // Public events
    Initialize,
    BeginOAuthFlow {
        scopes: Vec<String>,
    },
    BeginPairingFlow {
        pairing_url: String,
        scopes: Vec<String>,
    },
    CompleteOAuthFlow {
        code: String,
        state: String,
    },
    CancelOAuthFlow,
    CheckAuthorizationStatus,
    Disconnect,
    // Internal events
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
    CallError,
}

impl From<FxaState> for State {
    fn from(state: FxaState) -> State {
        match state {
            FxaState::Uninitialized => State::Uninitialized,
            FxaState::Disconnected => State::Disconnected,
            FxaState::Authenticating { oauth_url } => State::Authenticating { oauth_url },
            FxaState::Connected => State::Connected,
            FxaState::AuthIssues => State::AuthIssues,
        }
    }
}

impl From<FxaEvent> for Event {
    fn from(event: FxaEvent) -> Event {
        match event {
            FxaEvent::Initialize => Event::Initialize,
            FxaEvent::BeginOAuthFlow { scopes } => Event::BeginOAuthFlow { scopes },
            FxaEvent::BeginPairingFlow {
                pairing_url,
                scopes,
            } => Event::BeginPairingFlow {
                pairing_url,
                scopes,
            },
            FxaEvent::CompleteOAuthFlow { code, state } => Event::CompleteOAuthFlow { code, state },
            FxaEvent::CancelOAuthFlow => Event::CancelOAuthFlow,
            FxaEvent::CheckAuthorizationStatus => Event::CheckAuthorizationStatus,
            FxaEvent::Disconnect => Event::Disconnect,
        }
    }
}

// Try to convert a [State] to a [FxaState]
//
// On error, return the original state back
impl TryFrom<State> for FxaState {
    type Error = State;

    fn try_from(state: State) -> Result<FxaState, State> {
        match state {
            State::Disconnected => Ok(FxaState::Disconnected),
            State::Authenticating { oauth_url } => Ok(FxaState::Authenticating { oauth_url }),
            State::Connected => Ok(FxaState::Connected),
            State::AuthIssues => Ok(FxaState::AuthIssues),
            _ => Err(state),
        }
    }
}

// Try to convert a [Event] to a [FxaEvent]
//
// On error, return the original event back
impl TryFrom<Event> for FxaEvent {
    type Error = Event;

    fn try_from(event: Event) -> Result<FxaEvent, Event> {
        match event {
            Event::BeginOAuthFlow { scopes } => Ok(FxaEvent::BeginOAuthFlow { scopes }),
            _ => Err(event),
        }
    }
}

// Display impl for State
//
// This only returns the variant name to avoid leaking any PII
impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::Uninitialized { .. } => "State::Uninitialized",
            Self::Disconnected { .. } => "State::Disconnected",
            Self::Authenticating { .. } => "State::Authenticating",
            Self::Connected { .. } => "State::Connected",
            Self::AuthIssues { .. } => "State::AuthIssues",
            Self::GetAuthState { .. } => "State::GetAuthState",
            Self::BeginOAuthFlow { .. } => "State::BeginOAuthFlow",
            Self::BeginPairingFlow { .. } => "State::BeginPairingFlow",
            Self::CompleteOAuthFlow { .. } => "State::CompleteOAuthFlow",
            Self::InitializeDevice { .. } => "State::InitializeDevice",
            Self::EnsureDeviceCapabilities { .. } => "State::EnsureDeviceCapabilities",
            Self::CheckAuthorizationStatus { .. } => "State::CheckAuthorizationStatus",
            Self::Disconnect { .. } => "State::Disconnect",
        };
        write!(f, "{name}")
    }
}

// Display impl for Event
//
// This only returns the variant name to avoid leaking any PII
impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::Initialize { .. } => "Event::Initialize",
            Self::BeginOAuthFlow { .. } => "Event::BeginOAuthFlow",
            Self::BeginPairingFlow { .. } => "Event::BeginPairingFlow",
            Self::CompleteOAuthFlow { .. } => "Event::CompleteOAuthFlow",
            Self::CancelOAuthFlow { .. } => "Event::CancelOAuthFlow",
            Self::CheckAuthorizationStatus { .. } => "Event::CheckAuthorizationStatus",
            Self::Disconnect { .. } => "Event::Disconnect",
            Self::GetAuthStateSuccess { .. } => "Event::GetAuthStateSuccess",
            Self::BeginOAuthFlowSuccess { .. } => "Event::BeginOAuthFlowSuccess",
            Self::BeginPairingFlowSuccess { .. } => "Event::BeginPairingFlowSuccess",
            Self::CompleteOAuthFlowSuccess { .. } => "Event::CompleteOAuthFlowSuccess",
            Self::InitializeDeviceSuccess { .. } => "Event::InitializeDeviceSuccess",
            Self::EnsureDeviceCapabilitiesSuccess { .. } => {
                "Event::EnsureDeviceCapabilitiesSuccess"
            }
            Self::CheckAuthorizationStatusSuccess { .. } => {
                "Event::CheckAuthorizationStatusSuccess"
            }
            Self::DisconnectSuccess { .. } => "Event::DisconnectSuccess",
            Self::CallError { .. } => "Event::CallError",
        };
        write!(f, "{name}")
    }
}

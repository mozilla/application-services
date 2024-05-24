/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Display impls for state machine types
//!
//! These are sent to Sentry, so they must not leak PII.
//! In general this means they don't output values for inner fields.
//!
//! Also, they must not use the string "auth" since Sentry will filter that out.
//! Use "ath" instead.

use super::{internal_machines, FxaEvent, FxaState};
use std::fmt;

impl fmt::Display for FxaState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::Uninitialized => "Uninitialized",
            Self::Disconnected => "Disconnected",
            Self::Authenticating { .. } => "Athenticating",
            Self::Connected => "Connected",
            Self::AuthIssues => "AthIssues",
        };
        write!(f, "{name}")
    }
}

impl fmt::Display for FxaEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::Initialize { .. } => "Initialize",
            Self::BeginOAuthFlow { .. } => "BeginOAthFlow",
            Self::BeginPairingFlow { .. } => "BeginPairingFlow",
            Self::CompleteOAuthFlow { .. } => "CompleteOAthFlow",
            Self::CancelOAuthFlow => "CancelOAthFlow",
            Self::CheckAuthorizationStatus => "CheckAuthorizationStatus",
            Self::Disconnect => "Disconnect",
            Self::CallGetProfile => "CallGetProfile",
        };
        write!(f, "{name}")
    }
}

impl fmt::Display for internal_machines::State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::GetAuthState => write!(f, "GetAthState"),
            Self::BeginOAuthFlow { .. } => write!(f, "BeginOAthFlow"),
            Self::BeginPairingFlow { .. } => write!(f, "BeginPairingFlow"),
            Self::CompleteOAuthFlow { .. } => write!(f, "CompleteOAthFlow"),
            Self::InitializeDevice => write!(f, "InitializeDevice"),
            Self::EnsureDeviceCapabilities => write!(f, "EnsureDeviceCapabilities"),
            Self::CheckAuthorizationStatus => write!(f, "CheckAuthorizationStatus"),
            Self::Disconnect => write!(f, "Disconnect"),
            Self::GetProfile => write!(f, "GetProfile"),
            Self::Complete(state) => write!(f, "Complete({state})"),
            Self::Cancel => write!(f, "Cancel"),
        }
    }
}

impl fmt::Display for internal_machines::Event {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::GetAuthStateSuccess { .. } => "GetAthStateSuccess",
            Self::BeginOAuthFlowSuccess { .. } => "BeginOAthFlowSuccess",
            Self::BeginPairingFlowSuccess { .. } => "BeginPairingFlowSuccess",
            Self::CompleteOAuthFlowSuccess => "CompleteOAthFlowSuccess",
            Self::InitializeDeviceSuccess => "InitializeDeviceSuccess",
            Self::EnsureDeviceCapabilitiesSuccess => "EnsureDeviceCapabilitiesSuccess",
            Self::CheckAuthorizationStatusSuccess { .. } => "CheckAuthorizationStatusSuccess",
            Self::DisconnectSuccess => "DisconnectSuccess",
            Self::GetProfileSuccess => "GetProfileSuccess",
            Self::CallError => "CallError",
            Self::EnsureCapabilitiesAuthError => "EnsureCapabilitiesAthError",
        };
        write!(f, "{name}")
    }
}

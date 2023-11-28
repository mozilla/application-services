/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Display impls for state machine types
//!
//! These are sent to Sentry, so they must not leak PII.
//! In general this means they don't output values for inner fields.

use super::{internal_machines, FxaEvent, FxaState};
use std::fmt;

impl fmt::Display for FxaState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::Uninitialized => "Uninitialized",
            Self::Disconnected => "Disconnected",
            Self::Authenticating { .. } => "Authenticating",
            Self::Connected => "Connected",
            Self::AuthIssues => "AuthIssues",
        };
        write!(f, "{name}")
    }
}

impl fmt::Display for FxaEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::Initialize { .. } => "Initialize",
            Self::BeginOAuthFlow { .. } => "BeginOAuthFlow",
            Self::BeginPairingFlow { .. } => "BeginPairingFlow",
            Self::CompleteOAuthFlow { .. } => "CompleteOAuthFlow",
            Self::CancelOAuthFlow => "CancelOAuthFlow",
            Self::CheckAuthorizationStatus => "CheckAuthorizationStatus",
            Self::Disconnect => "Disconnect",
        };
        write!(f, "{name}")
    }
}

impl fmt::Display for internal_machines::State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::GetAuthState => "GetAuthState",
            Self::BeginOAuthFlow { .. } => "BeginOAuthFlow",
            Self::BeginPairingFlow { .. } => "BeginPairingFlow",
            Self::CompleteOAuthFlow { .. } => "CompleteOAuthFlow",
            Self::InitializeDevice => "InitializeDevice",
            Self::EnsureDeviceCapabilities => "EnsureDeviceCapabilities",
            Self::CheckAuthorizationStatus => "CheckAuthorizationStatus",
            Self::Disconnect => "Disconnect",
            Self::Complete(_) => "Complete",
            Self::Cancel => "Cancel",
        };
        write!(f, "{name}")
    }
}

impl fmt::Display for internal_machines::Event {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = match self {
            Self::GetAuthStateSuccess { .. } => "GetAuthStateSuccess",
            Self::BeginOAuthFlowSuccess { .. } => "BeginOAuthFlowSuccess",
            Self::BeginPairingFlowSuccess { .. } => "BeginPairingFlowSuccess",
            Self::CompleteOAuthFlowSuccess => "CompleteOAuthFlowSuccess",
            Self::InitializeDeviceSuccess => "InitializeDeviceSuccess",
            Self::EnsureDeviceCapabilitiesSuccess => "EnsureDeviceCapabilitiesSuccess",
            Self::CheckAuthorizationStatusSuccess { .. } => "CheckAuthorizationStatusSuccess",
            Self::DisconnectSuccess => "DisconnectSuccess",
            Self::CallError => "CallError",
            Self::EnsureCapabilitiesAuthError => "EnsureCapabilitiesAuthError",
        };
        write!(f, "{name}")
    }
}

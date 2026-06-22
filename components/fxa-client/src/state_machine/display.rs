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

use super::{FxaEvent, FxaState};
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
            Self::WebChannelPasswordChange { .. } => "WebChannelPwdChange",
            Self::Disconnect => "Disconnect",
            Self::CallGetProfile => "CallGetProfile",
        };
        write!(f, "{name}")
    }
}

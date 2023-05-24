/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use thiserror::Error;

/// Generic error type thrown by many [`FirefoxAccount`] operations.
///
/// Precise details of the error are hidden from consumers, mostly due to limitations of
/// how we expose this API to other languages. The type of the error indicates how the
/// calling code should respond.
#[derive(Debug, Error)]
pub enum FxaError {
    /// Thrown when there was a problem with the authentication status of the account,
    /// such as an expired token. The application should [check its authorization status](
    /// FirefoxAccount::check_authorization_status) to see whether it has been disconnected,
    /// or retry the operation with a freshly-generated token.
    #[error("authentication error")]
    Authentication,
    /// Thrown if an operation fails due to network access problems.
    /// The application may retry at a later time once connectivity is restored.
    #[error("network error")]
    Network,
    /// Thrown if the application attempts to complete an OAuth flow when no OAuth flow
    /// has been initiated. This may indicate a user who navigated directly to the OAuth
    /// `redirect_uri` for the application.
    ///
    /// **Note:** This error is currently only thrown in the Swift language bindings.
    #[error("no authentication flow was active")]
    NoExistingAuthFlow,
    /// Thrown if the application attempts to complete an OAuth flow, but the state
    /// tokens returned from the Firefox Account server do not match with the ones
    /// expected by the client.
    /// This may indicate a stale OAuth flow, or potentially an attempted hijacking
    /// of the flow by an attacker. The signin attempt cannot be completed.
    ///
    /// **Note:** This error is currently only thrown in the Swift language bindings.
    #[error("the requested authentication flow was not active")]
    WrongAuthFlow,
    /// Thrown if there is a panic in the underlying Rust code.
    ///
    /// **Note:** This error is currently only thrown in the Kotlin language bindings.
    #[error("panic in native code")]
    Panic,
    /// A catch-all for other unspecified errors.
    #[error("other error")]
    Other,
}

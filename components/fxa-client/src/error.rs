/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use error_support::{ErrorHandling, GetErrorHandling};
use rc_crypto::hawk;
use std::string;

/// Public error type thrown by many [`FirefoxAccount`] operations.
///
/// Precise details of the error are hidden from consumers. The type of the error indicates how the
/// calling code should respond.
#[derive(Debug, thiserror::Error)]
pub enum FxaError {
    /// Thrown when there was a problem with the authentication status of the account,
    /// such as an expired token. The application should [check its authorization status](
    /// FirefoxAccount::check_authorization_status) to see whether it has been disconnected,
    /// or retry the operation with a freshly-generated token.
    #[error("authentication error")]
    AuthError,
    /// Thrown if an operation fails due to network access problems.
    /// The application may retry at a later time once connectivity is restored.
    #[error("network error")]
    NetworkError,
    /// Attempt to call a method that's not valid for this particular state (for example
    /// `begin_oauth_flow()` when the client is already logged in
    #[error("{method} can not be called from the {state} state")]
    InvalidState {
        method: String,
        state: String,
    },
    /// A catch-all for other unspecified errors.
    #[error("unexpected error: {0}")]
    UnexpectedError(String),
}

/// FxA internal error type
/// These are used in the internal code. This error type is never returned to the consumer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Server asked the client to back off, please wait {0} seconds to try again")]
    BackoffError(u64),

    #[error("Unknown OAuth State")]
    UnknownOAuthState,

    #[error("Multiple OAuth scopes requested")]
    MultipleScopesRequested,

    #[error("No cached token for scope {0}")]
    NoCachedToken(String),

    #[error("No cached scoped keys for scope {0}")]
    NoScopedKey(String),

    #[error("No stored refresh token")]
    NoRefreshToken,

    #[error("No stored session token")]
    NoSessionToken,

    #[error("No stored migration data")]
    NoMigrationData,

    #[error("No stored current device id")]
    NoCurrentDeviceId,

    #[error("Device target is unknown (Device ID: {0})")]
    UnknownTargetDevice(String),

    #[error("Unrecoverable server error {0}")]
    UnrecoverableServerError(&'static str),

    #[error("Illegal state: {0}")]
    IllegalState(&'static str),

    #[error("Unknown command: {0}")]
    UnknownCommand(String),

    #[error("Send Tab diagnosis error: {0}")]
    SendTabDiagnosisError(&'static str),

    #[error("Cannot xor arrays with different lengths: {0} and {1}")]
    XorLengthMismatch(usize, usize),

    #[error("Origin mismatch")]
    OriginMismatch,

    #[error("Remote key and local key mismatch")]
    MismatchedKeys,

    #[error("Client: {0} is not allowed to request scope: {1}")]
    ScopeNotAllowed(String, String),

    #[error("Unsupported command: {0}")]
    UnsupportedCommand(&'static str),

    #[error("Missing URL parameter: {0}")]
    MissingUrlParameter(&'static str),

    #[error("Null pointer passed to FFI")]
    NullPointer,

    #[error("Invalid buffer length: {0}")]
    InvalidBufferLength(i32),

    #[error("Too many calls to auth introspection endpoint")]
    AuthCircuitBreakerError,

    #[error("Remote server error: '{code}' '{errno}' '{error}' '{message}' '{info}'")]
    RemoteError {
        code: u64,
        errno: u64,
        error: String,
        message: String,
        info: String,
    },

    // Basically reimplement error_chain's foreign_links. (Ugh, this sucks).
    #[error("Crypto/NSS error: {0}")]
    CryptoError(#[from] rc_crypto::Error),

    #[error("http-ece encryption error: {0}")]
    EceError(#[from] rc_crypto::ece::Error),

    #[error("Hex decode error: {0}")]
    HexDecodeError(#[from] hex::FromHexError),

    #[error("Base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("JWCrypto error: {0}")]
    JwCryptoError(#[from] jwcrypto::JwCryptoError),

    #[error("UTF8 decode error: {0}")]
    UTF8DecodeError(#[from] string::FromUtf8Error),

    #[error("Network error: {0}")]
    RequestError(#[from] viaduct::Error),

    #[error("Malformed URL error: {0}")]
    MalformedUrl(#[from] url::ParseError),

    #[error("Unexpected HTTP status: {0}")]
    UnexpectedStatus(#[from] viaduct::UnexpectedStatus),

    #[error("Sync15 error: {0}")]
    SyncError(#[from] sync15::Error),

    #[error("HAWK error: {0}")]
    HawkError(#[from] hawk::Error),

    #[error("Integer conversion error: {0}")]
    IntegerConversionError(#[from] std::num::TryFromIntError),

    #[error("Command not found by fxa")]
    CommandNotFound,

    #[error("Invalid Push Event")]
    InvalidPushEvent,
}

// Define how our internal errors are handled and converted to external errors
// See `support/error/README.md` for how this works, especially the warning about PII.
impl GetErrorHandling for Error {
    type ExternalError = FxaError;

    fn get_error_handling(&self) -> ErrorHandling<Self::ExternalError> {
        match self {
            Error::RemoteError { code: 401, .. }
            | Error::NoRefreshToken
            | Error::NoScopedKey(_)
            | Error::NoCachedToken(_) => {
                ErrorHandling::convert(crate::FxaError::AuthError).log_warning()
            }
            Error::RequestError(_) => {
                ErrorHandling::convert(crate::FxaError::NetworkError).log_warning()
            }
            _ => ErrorHandling::convert(crate::FxaError::UnexpectedError(self.to_string())).log_warning(),
        }
    }
}

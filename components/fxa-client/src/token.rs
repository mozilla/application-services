/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Token Management
//!
//! A signed-in application will typically hold a number of different *tokens* associated with the
//! user's account, including:
//!
//!    - An OAuth `refresh_token`, representing their ongoing connection to the account
//!      and the scopes that have been granted.
//!    - Short-lived OAuth `access_token`s that can be used to access resources on behalf
//!      of the user.
//!    - Optionally, a `session_token` that gives full control over the user's account,
//!      typically managed on behalf of web content that runs within the context
//!      of the application.

use crate::{ApiResult, Error, FirefoxAccount};
use error_support::handle_error;
use serde_derive::*;
use std::convert::{TryFrom, TryInto};

impl FirefoxAccount {
    /// Get a short-lived OAuth access token for the user's account.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications that need to access resources on behalf of the user must obtain an
    /// `access_token` in order to do so. For example, an access token is required when
    /// fetching the user's profile data, or when accessing their data stored in Firefox Sync.
    ///
    /// This method will obtain and return an access token bearing the requested scopes, either
    /// from a local cache of previously-issued tokens, or by creating a new one from the server.
    ///
    /// # Arguments
    ///
    ///    - `scope` - the OAuth scope to be granted by the token.
    ///        - This must be one of the scopes requested during the signin flow.
    ///        - Only a single scope is supported; for multiple scopes request multiple tokens.
    ///    - `ttl` - optionally, the time for which the token should be valid, in seconds.
    ///
    /// # Notes
    ///
    ///    - If the application receives an authorization error when trying to use the resulting
    ///      token, it should call [`clear_access_token_cache`](FirefoxAccount::clear_access_token_cache)
    ///      before requesting a fresh token.
    #[handle_error(Error)]
    pub fn get_access_token(&self, scope: &str, ttl: Option<i64>) -> ApiResult<AccessTokenInfo> {
        // Signedness converstion for Kotlin compatibility :-/
        let ttl = ttl.map(|ttl| u64::try_from(ttl).unwrap_or_default());
        self.internal
            .lock()
            .get_access_token(scope, ttl)?
            .try_into()
    }

    /// Get the session token for the user's account, if one is available.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications that function as a web browser may need to hold on to a session token
    /// on behalf of Firefox Accounts web content. This method exists so that they can retrieve
    /// it an pass it back to said web content when required.
    ///
    /// # Notes
    ///
    ///    - Please do not attempt to use the resulting token to directly make calls to the
    ///      Firefox Accounts servers! All account management functionality should be performed
    ///      in web content.
    ///    - A session token is only available to applications that have requested the
    ///      `https://identity.mozilla.com/tokens/session` scope.
    #[handle_error(Error)]
    pub fn get_session_token(&self) -> ApiResult<String> {
        self.internal.lock().get_session_token()
    }

    /// Update the stored session token for the user's account.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications that function as a web browser may need to hold on to a session token
    /// on behalf of Firefox Accounts web content. This method exists so that said web content
    /// signals that it has generated a new session token, the stored value can be updated
    /// to match.
    ///
    /// # Arguments
    ///
    ///    - `session_token` - the new session token value provided from web content.
    #[handle_error(Error)]
    pub fn handle_session_token_change(&self, session_token: &str) -> ApiResult<()> {
        self.internal
            .lock()
            .handle_session_token_change(session_token)
    }

    /// Create a new OAuth authorization code using the stored session token.
    ///
    /// When a signed-in application receives an incoming device pairing request, it can
    /// use this method to grant the request and generate a corresponding OAuth authorization
    /// code. This code would then be passed back to the connecting device over the
    /// pairing channel (a process which is not currently supported by any code in this
    /// component).
    ///
    /// # Arguments
    ///
    ///    - `params` - the OAuth parameters from the incoming authorization request
    #[handle_error(Error)]
    pub fn authorize_code_using_session_token(
        &self,
        params: AuthorizationParameters,
    ) -> ApiResult<String> {
        self.internal
            .lock()
            .authorize_code_using_session_token(params)
    }

    /// Clear the access token cache in response to an auth failure.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// Applications that receive an authentication error when trying to use an access token,
    /// should call this method before creating a new token and retrying the failed operation.
    /// It ensures that the expired token is removed and a fresh one generated.
    pub fn clear_access_token_cache(&self) {
        self.internal.lock().clear_access_token_cache()
    }
}

/// An OAuth access token, with its associated keys and metadata.
///
/// This struct represents an FxA OAuth access token, which can be used to access a resource
/// or service on behalf of the user. For example, accessing the user's data in Firefox Sync
/// an access token for the scope `https://identity.mozilla.com/apps/sync` along with the
/// associated encryption key.
#[derive(Debug)]
pub struct AccessTokenInfo {
    /// The scope of access granted by token.
    pub scope: String,
    /// The access token itself.
    ///
    /// This is the value that should be included in the `Authorization` header when
    /// accessing an OAuth protected resource on behalf of the user.
    pub token: String,
    /// The client-side encryption key associated with this scope.
    ///
    /// **⚠️ Warning:** the value of this field should never be revealed outside of the
    /// application. For example, it should never to sent to a server or logged in a log file.
    pub key: Option<ScopedKey>,
    /// The expiry time of the token, in seconds.
    ///
    /// This is the timestamp at which the token is set to expire, in seconds since
    /// unix epoch. Note that it is a signed integer, for compatibility with languages
    /// that do not have an unsigned integer type.
    ///
    /// This timestamp is for guidance only. Access tokens are not guaranteed to remain
    /// value for any particular lengthof time, and consumers should be prepared to handle
    /// auth failures even if the token has not yet expired.
    pub expires_at: i64,
}

/// A cryptographic key associated with an OAuth scope.
///
/// Some OAuth scopes have a corresponding client-side encryption key that is required
/// in order to access protected data. This struct represents such key material in a
/// format compatible with the common "JWK" standard.
///
#[derive(Clone, Serialize, Deserialize)]
pub struct ScopedKey {
    /// The type of key.
    ///
    /// In practice for FxA, this will always be string string "oct" (short for "octal")
    /// to represent a raw symmetric key.
    pub kty: String,
    /// The OAuth scope with which this key is associated.
    pub scope: String,
    /// The key material, as base64-url-encoded bytes.
    ///
    /// **⚠️ Warning:** the value of this field should never be revealed outside of the
    /// application. For example, it should never to sent to a server or logged in a log file.
    pub k: String,
    /// An opaque unique identifier for this key.
    ///
    /// Unlike the `k` field, this value is not secret and may be revealed to the server.
    pub kid: String,
}

/// Parameters provided in an incoming OAuth request.
///
/// This struct represents parameters obtained from an incoming OAuth request - that is,
/// the values that an OAuth client would append to the authorization URL when initiating
/// an OAuth sign-in flow.
pub struct AuthorizationParameters {
    pub client_id: String,
    pub scope: Vec<String>,
    pub state: String,
    pub access_type: String,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub keys_jwk: Option<String>,
}

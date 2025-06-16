/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub mod attached_clients;
use super::scopes;
use super::{
    http_client::{
        AuthorizationRequestParameters, IntrospectResponse as IntrospectInfo, OAuthTokenResponse,
    },
    scoped_keys::ScopedKeysFlow,
    util, FirefoxAccount,
};
use crate::auth::UserData;
use crate::{warn, AuthorizationParameters, Error, FxaServer, Result, ScopedKey};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jwcrypto::{EncryptionAlgorithm, EncryptionParameters};
use rate_limiter::RateLimiter;
use rc_crypto::digest;
use serde_derive::*;
use std::{
    collections::{HashMap, HashSet},
    iter::FromIterator,
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;
// If a cached token has less than `OAUTH_MIN_TIME_LEFT` seconds left to live,
// it will be considered already expired.
const OAUTH_MIN_TIME_LEFT: u64 = 60;
// Special redirect urn based on the OAuth native spec, signals that the
// WebChannel flow is used
pub const OAUTH_WEBCHANNEL_REDIRECT: &str = "urn:ietf:wg:oauth:2.0:oob:oauth-redirect-webchannel";

impl FirefoxAccount {
    /// Fetch a short-lived access token using the saved refresh token.
    /// If there is no refresh token held or if it is not authorized for some of the requested
    /// scopes, this method will error-out and a login flow will need to be initiated
    /// using `begin_oauth_flow`.
    ///
    /// * `scopes` - Space-separated list of requested scopes.
    /// * `ttl` - the ttl in seconds of the token requested from the server.
    ///
    /// **💾 This method may alter the persisted account state.**
    pub fn get_access_token(&mut self, scope: &str, ttl: Option<u64>) -> Result<AccessTokenInfo> {
        if scope.contains(' ') {
            return Err(Error::MultipleScopesRequested);
        }
        if let Some(oauth_info) = self.state.get_cached_access_token(scope) {
            if oauth_info.expires_at > util::now_secs() + OAUTH_MIN_TIME_LEFT {
                // If the cached key is missing the required sync scoped key, try to fetch it again
                if oauth_info.check_missing_sync_scoped_key().is_ok() {
                    return Ok(oauth_info.clone());
                }
            }
        }
        let resp = match self.state.refresh_token() {
            Some(refresh_token) => {
                if refresh_token.scopes.contains(scope) {
                    self.client.create_access_token_using_refresh_token(
                        self.state.config(),
                        &refresh_token.token,
                        ttl,
                        &[scope],
                    )?
                } else {
                    return Err(Error::NoCachedToken(scope.to_string()));
                }
            }
            None => match self.state.session_token() {
                Some(session_token) => self.client.create_access_token_using_session_token(
                    self.state.config(),
                    session_token,
                    &[scope],
                )?,
                None => return Err(Error::NoCachedToken(scope.to_string())),
            },
        };
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::IllegalState("Current date before Unix Epoch."))?;
        let expires_at = since_epoch.as_secs() + resp.expires_in;
        let token_info = AccessTokenInfo {
            scope: resp.scope,
            token: resp.access_token,
            key: self.state.get_scoped_key(scope).cloned(),
            expires_at,
        };
        self.state
            .add_cached_access_token(scope, token_info.clone());
        token_info.check_missing_sync_scoped_key()?;
        Ok(token_info)
    }

    /// Sets the user data (session token, email, uid)
    pub fn set_user_data(&mut self, user_data: UserData) {
        // for now, we only have use for the session token
        // if we'd like to implement a "Signed in but not verified" state
        // we would also consume the other parts of the user data
        self.state.set_session_token(user_data.session_token)
    }

    /// Retrieve the current session token from state
    pub fn get_session_token(&self) -> Result<String> {
        match self.state.session_token() {
            Some(session_token) => Ok(session_token.to_string()),
            None => Err(Error::NoSessionToken),
        }
    }

    /// Check whether user is authorized using our refresh token.
    pub fn check_authorization_status(&mut self) -> Result<IntrospectInfo> {
        let resp = match self.state.refresh_token() {
            Some(refresh_token) => {
                self.auth_circuit_breaker.check()?;
                self.client
                    .check_refresh_token_status(self.state.config(), &refresh_token.token)?
            }
            None => return Err(Error::NoRefreshToken),
        };
        Ok(IntrospectInfo {
            active: resp.active,
        })
    }

    /// Initiate a pairing flow and return a URL that should be navigated to.
    ///
    /// * `pairing_url` - A pairing URL obtained by scanning a QR code produced by
    ///   the pairing authority.
    /// * `scopes` - Space-separated list of requested scopes by the pairing supplicant.
    /// * `entrypoint` - The entrypoint to be used for data collection
    /// * `metrics` - Optional parameters for metrics
    pub fn begin_pairing_flow(
        &mut self,
        pairing_url: &str,
        scopes: &[&str],
        entrypoint: &str,
    ) -> Result<String> {
        let mut url = self.state.config().pair_supp_url()?;
        url.query_pairs_mut().append_pair("entrypoint", entrypoint);
        let pairing_url = Url::parse(pairing_url)?;
        if url.host_str() != pairing_url.host_str() {
            let fxa_server = FxaServer::from(&url);
            let pairing_fxa_server = FxaServer::from(&pairing_url);
            return Err(Error::OriginMismatch(format!(
                "fxa-server: {fxa_server}, pairing-url-fxa-server: {pairing_fxa_server}"
            )));
        }
        url.set_fragment(pairing_url.fragment());
        self.oauth_flow(url, scopes)
    }

    /// Initiate an OAuth login flow and return a URL that should be navigated to.
    ///
    /// * `scopes` - Space-separated list of requested scopes.
    /// * `entrypoint` - The entrypoint to be used for metrics
    /// * `metrics` - Optional metrics parameters
    pub fn begin_oauth_flow(&mut self, scopes: &[&str], entrypoint: &str) -> Result<String> {
        self.state.on_begin_oauth();
        let mut url = if self.state.last_seen_profile().is_some() {
            self.state.config().oauth_force_auth_url()?
        } else {
            self.state.config().authorization_endpoint()?
        };

        url.query_pairs_mut()
            .append_pair("action", "email")
            .append_pair("response_type", "code")
            .append_pair("entrypoint", entrypoint);

        if let Some(cached_profile) = self.state.last_seen_profile() {
            url.query_pairs_mut()
                .append_pair("email", &cached_profile.response.email);
        }

        let scopes: Vec<String> = match self.state.refresh_token() {
            Some(refresh_token) => {
                // Union of the already held scopes and the one requested.
                let mut all_scopes: Vec<String> = vec![];
                all_scopes.extend(scopes.iter().map(ToString::to_string));
                let existing_scopes = refresh_token.scopes.clone();
                all_scopes.extend(existing_scopes);
                HashSet::<String>::from_iter(all_scopes)
                    .into_iter()
                    .collect()
            }
            None => scopes.iter().map(ToString::to_string).collect(),
        };
        let scopes: Vec<&str> = scopes.iter().map(<_>::as_ref).collect();
        self.oauth_flow(url, &scopes)
    }

    /// Fetch an OAuth code for a particular client using a session token from the account state.
    ///
    /// * `auth_params` Authorization parameters  which includes:
    ///     *  `client_id` - OAuth client id.
    ///     *  `scope` - list of requested scopes.
    ///     *  `state` - OAuth state.
    ///     *  `access_type` - Type of OAuth access, can be "offline" and "online"
    ///     *  `pkce_params` - Optional PKCE parameters for public clients (`code_challenge` and `code_challenge_method`)
    ///     *  `keys_jwk` - Optional JWK used to encrypt scoped keys
    pub fn authorize_code_using_session_token(
        &self,
        auth_params: AuthorizationParameters,
    ) -> Result<String> {
        let session_token = self.get_session_token()?;

        // Validate request to ensure that the client is actually allowed to request
        // the scopes they requested
        let allowed_scopes = self.client.get_scoped_key_data(
            self.state.config(),
            &session_token,
            &auth_params.client_id,
            &auth_params.scope.join(" "),
        )?;

        if let Some(not_allowed_scope) = auth_params
            .scope
            .iter()
            .find(|scope| !allowed_scopes.contains_key(*scope))
        {
            return Err(Error::ScopeNotAllowed(
                auth_params.client_id.clone(),
                not_allowed_scope.clone(),
            ));
        }

        let keys_jwe = if let Some(keys_jwk) = auth_params.keys_jwk {
            let mut scoped_keys = HashMap::new();
            allowed_scopes
                .iter()
                .try_for_each(|(scope, _)| -> Result<()> {
                    scoped_keys.insert(
                        scope,
                        self.state
                            .get_scoped_key(scope)
                            .ok_or_else(|| Error::NoScopedKey(scope.clone()))?,
                    );
                    Ok(())
                })?;
            let scoped_keys = serde_json::to_string(&scoped_keys)?;
            let keys_jwk = URL_SAFE_NO_PAD.decode(keys_jwk)?;
            let jwk = serde_json::from_slice(&keys_jwk)?;
            Some(jwcrypto::encrypt_to_jwe(
                scoped_keys.as_bytes(),
                EncryptionParameters::ECDH_ES {
                    enc: EncryptionAlgorithm::A256GCM,
                    peer_jwk: &jwk,
                },
            )?)
        } else {
            None
        };
        let auth_request_params = AuthorizationRequestParameters {
            client_id: auth_params.client_id,
            scope: auth_params.scope.join(" "),
            state: auth_params.state,
            access_type: auth_params.access_type,
            code_challenge: auth_params.code_challenge,
            code_challenge_method: auth_params.code_challenge_method,
            keys_jwe,
        };

        let resp = self.client.create_authorization_code_using_session_token(
            self.state.config(),
            &session_token,
            auth_request_params,
        )?;

        Ok(resp.code)
    }

    fn oauth_flow(&mut self, mut url: Url, scopes: &[&str]) -> Result<String> {
        self.clear_access_token_cache();
        let state = util::random_base64_url_string(16)?;
        let code_verifier = util::random_base64_url_string(43)?;
        let code_challenge = digest::digest(&digest::SHA256, code_verifier.as_bytes())?;
        let code_challenge = URL_SAFE_NO_PAD.encode(code_challenge);
        let scoped_keys_flow = ScopedKeysFlow::with_random_key()?;
        let jwk = scoped_keys_flow.get_public_key_jwk()?;
        let jwk_json = serde_json::to_string(&jwk)?;
        let keys_jwk = URL_SAFE_NO_PAD.encode(jwk_json);
        url.query_pairs_mut()
            .append_pair("client_id", &self.state.config().client_id)
            .append_pair("scope", &scopes.join(" "))
            .append_pair("state", &state)
            .append_pair("code_challenge_method", "S256")
            .append_pair("code_challenge", &code_challenge)
            .append_pair("access_type", "offline")
            .append_pair("keys_jwk", &keys_jwk);

        if self.state.config().redirect_uri == OAUTH_WEBCHANNEL_REDIRECT {
            url.query_pairs_mut()
                .append_pair("context", "oauth_webchannel_v1");
        } else {
            url.query_pairs_mut()
                .append_pair("redirect_uri", &self.state.config().redirect_uri);
        }

        self.state.begin_oauth_flow(
            state,
            OAuthFlow {
                scoped_keys_flow: Some(scoped_keys_flow),
                code_verifier,
            },
        );
        Ok(url.to_string())
    }

    /// Complete an OAuth flow initiated in `begin_oauth_flow` or `begin_pairing_flow`.
    /// The `code` and `state` parameters can be obtained by parsing out the
    /// redirect URL after a successful login.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn complete_oauth_flow(&mut self, code: &str, state: &str) -> Result<()> {
        self.clear_access_token_cache();
        let oauth_flow = match self.state.pop_oauth_flow(state) {
            Some(oauth_flow) => oauth_flow,
            None => return Err(Error::UnknownOAuthState),
        };
        let resp = self.client.create_refresh_token_using_authorization_code(
            self.state.config(),
            self.state.session_token(),
            code,
            &oauth_flow.code_verifier,
        )?;
        self.handle_oauth_response(resp, oauth_flow.scoped_keys_flow)
    }

    pub(crate) fn handle_oauth_response(
        &mut self,
        resp: OAuthTokenResponse,
        scoped_keys_flow: Option<ScopedKeysFlow>,
    ) -> Result<()> {
        let sync_scope_granted = resp.scope.split(' ').any(|s| s == scopes::OLD_SYNC);
        let scoped_keys = match resp.keys_jwe {
            Some(ref jwe) => {
                let scoped_keys_flow = scoped_keys_flow.ok_or(Error::ApiClientError(
                    "Got a JWE but have no JWK to decrypt it.",
                ))?;
                let decrypted_keys = scoped_keys_flow.decrypt_keys_jwe(jwe)?;
                let scoped_keys: serde_json::Map<String, serde_json::Value> =
                    serde_json::from_str(&decrypted_keys)?;
                if sync_scope_granted && !scoped_keys.contains_key(scopes::OLD_SYNC) {
                    error_support::report_error!(
                        "fxaclient-scoped-key",
                        "Sync scope granted, but no sync scoped key (scope granted: {}, key scopes: {})",
                        resp.scope,
                        scoped_keys.keys().map(|s| s.as_ref()).collect::<Vec<&str>>().join(", ")
                    );
                }
                scoped_keys
                    .into_iter()
                    .map(|(scope, key)| Ok((scope, serde_json::from_value(key)?)))
                    .collect::<Result<Vec<_>>>()?
            }
            None => {
                if sync_scope_granted {
                    error_support::report_error!(
                        "fxaclient-scoped-key",
                        "Sync scope granted, but keys_jwe is None"
                    );
                }
                vec![]
            }
        };

        // We are only interested in the refresh token at this time because we
        // don't want to return an over-scoped access token.
        // Let's be good citizens and destroy this access token.
        if let Err(err) = self
            .client
            .destroy_access_token(self.state.config(), &resp.access_token)
        {
            warn!("Access token destruction failure: {:?}", err);
        }
        let old_refresh_token = self.state.refresh_token().cloned();
        let new_refresh_token = resp
            .refresh_token
            .ok_or(Error::ApiClientError("No refresh token in response"))?;
        // Destroying a refresh token also destroys its associated device,
        // grab the device information for replication later.
        let old_device_info = match old_refresh_token {
            Some(_) => match self.get_current_device() {
                Ok(maybe_device) => maybe_device,
                Err(err) => {
                    warn!("Error while getting previous device information: {:?}", err);
                    None
                }
            },
            None => None,
        };
        // In order to keep 1 and only 1 refresh token alive per client instance,
        // we also destroy the existing refresh token.
        if let Some(ref refresh_token) = old_refresh_token {
            if let Err(err) = self
                .client
                .destroy_refresh_token(self.state.config(), &refresh_token.token)
            {
                warn!("Refresh token destruction failure: {:?}", err);
            }
        }
        if let Some(ref device_info) = old_device_info {
            if let Err(err) = self.replace_device(
                &device_info.display_name,
                &device_info.device_type,
                &device_info.push_subscription,
                &device_info.available_commands,
            ) {
                warn!("Device information restoration failed: {:?}", err);
            }
        }
        self.state.complete_oauth_flow(
            scoped_keys,
            RefreshToken {
                token: new_refresh_token,
                scopes: resp.scope.split(' ').map(ToString::to_string).collect(),
            },
            resp.session_token,
        );
        Ok(())
    }

    /// Typically called during a password change flow.
    /// Invalidates all tokens and fetches a new refresh token.
    /// Because the old refresh token is not valid anymore, we can't do like `handle_oauth_response`
    /// and re-create the device, so it is the responsibility of the caller to do so after we're
    /// done.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn handle_session_token_change(&mut self, session_token: &str) -> Result<()> {
        let old_refresh_token = self.state.refresh_token().ok_or(Error::NoRefreshToken)?;
        let scopes: Vec<&str> = old_refresh_token.scopes.iter().map(AsRef::as_ref).collect();
        let resp = self.client.create_refresh_token_using_session_token(
            self.state.config(),
            session_token,
            &scopes,
        )?;
        let new_refresh_token = resp
            .refresh_token
            .ok_or(Error::ApiClientError("No refresh token in response"))?;
        self.state.update_tokens(
            session_token.to_owned(),
            RefreshToken {
                token: new_refresh_token,
                scopes: resp.scope.split(' ').map(ToString::to_string).collect(),
            },
        );
        self.clear_devices_and_attached_clients_cache();
        Ok(())
    }

    /// **💾 This method may alter the persisted account state.**
    pub fn clear_access_token_cache(&mut self) {
        self.state.clear_access_token_cache();
    }
}

const AUTH_CIRCUIT_BREAKER_CAPACITY: u8 = 5;
const AUTH_CIRCUIT_BREAKER_RENEWAL_RATE: f32 = 3.0 / 60.0 / 1000.0; // 3 tokens every minute.

#[derive(Clone, Copy)]
pub(crate) struct AuthCircuitBreaker {
    rate_limiter: RateLimiter,
}

impl Default for AuthCircuitBreaker {
    fn default() -> Self {
        AuthCircuitBreaker {
            rate_limiter: RateLimiter::new(
                AUTH_CIRCUIT_BREAKER_CAPACITY,
                AUTH_CIRCUIT_BREAKER_RENEWAL_RATE,
            ),
        }
    }
}

impl AuthCircuitBreaker {
    pub(crate) fn check(&mut self) -> Result<()> {
        if !self.rate_limiter.check() {
            return Err(Error::AuthCircuitBreakerError);
        }
        Ok(())
    }
}

impl TryFrom<Url> for AuthorizationParameters {
    type Error = Error;

    fn try_from(url: Url) -> Result<Self> {
        let query_map: HashMap<String, String> = url.query_pairs().into_owned().collect();
        let scope = query_map
            .get("scope")
            .cloned()
            .ok_or(Error::MissingUrlParameter("scope"))?;
        let client_id = query_map
            .get("client_id")
            .cloned()
            .ok_or(Error::MissingUrlParameter("client_id"))?;
        let state = query_map
            .get("state")
            .cloned()
            .ok_or(Error::MissingUrlParameter("state"))?;
        let access_type = query_map
            .get("access_type")
            .cloned()
            .ok_or(Error::MissingUrlParameter("access_type"))?;
        let code_challenge = query_map.get("code_challenge").cloned();
        let code_challenge_method = query_map.get("code_challenge_method").cloned();
        let keys_jwk = query_map.get("keys_jwk").cloned();
        Ok(Self {
            client_id,
            scope: scope.split_whitespace().map(|s| s.to_string()).collect(),
            state,
            access_type,
            code_challenge,
            code_challenge_method,
            keys_jwk,
        })
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RefreshToken {
    pub token: String,
    pub scopes: HashSet<String>,
}

impl std::fmt::Debug for RefreshToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefreshToken")
            .field("scopes", &self.scopes)
            .finish()
    }
}

pub struct OAuthFlow {
    pub scoped_keys_flow: Option<ScopedKeysFlow>,
    pub code_verifier: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AccessTokenInfo {
    pub scope: String,
    pub token: String,
    pub key: Option<ScopedKey>,
    pub expires_at: u64, // seconds since epoch
}

impl AccessTokenInfo {
    pub fn check_missing_sync_scoped_key(&self) -> Result<()> {
        if self.scope == scopes::OLD_SYNC && self.key.is_none() {
            Err(Error::SyncScopedKeyMissingInServerResponse)
        } else {
            Ok(())
        }
    }
}

impl TryFrom<AccessTokenInfo> for crate::AccessTokenInfo {
    type Error = Error;
    fn try_from(info: AccessTokenInfo) -> Result<Self> {
        Ok(crate::AccessTokenInfo {
            scope: info.scope,
            token: info.token,
            key: info.key,
            expires_at: info.expires_at.try_into()?,
        })
    }
}

impl std::fmt::Debug for AccessTokenInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccessTokenInfo")
            .field("scope", &self.scope)
            .field("key", &self.key)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl From<IntrospectInfo> for crate::AuthorizationInfo {
    fn from(r: IntrospectInfo) -> Self {
        crate::AuthorizationInfo { active: r.active }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{http_client::*, Config};
    use super::*;
    use mockall::predicate::always;
    use mockall::predicate::eq;
    use std::borrow::Cow;
    use std::collections::HashMap;
    use std::sync::Arc;

    impl FirefoxAccount {
        pub fn add_cached_token(&mut self, scope: &str, token_info: AccessTokenInfo) {
            self.state.add_cached_access_token(scope, token_info);
        }

        pub fn set_session_token(&mut self, session_token: &str) {
            self.state.set_session_token(session_token.to_owned());
        }
    }

    #[test]
    fn test_oauth_flow_url() {
        nss::ensure_initialized();
        // FIXME: this test shouldn't make network requests.
        viaduct_reqwest::use_reqwest_backend();
        let config = Config::new(
            "https://accounts.firefox.com",
            "12345678",
            "https://foo.bar",
        );
        let mut fxa = FirefoxAccount::with_config(config);
        let url = fxa
            .begin_oauth_flow(&["profile"], "test_oauth_flow_url")
            .unwrap();
        let flow_url = Url::parse(&url).unwrap();

        assert_eq!(flow_url.host_str(), Some("accounts.firefox.com"));
        assert_eq!(flow_url.path(), "/authorization");

        let mut pairs = flow_url.query_pairs();
        assert_eq!(pairs.count(), 11);
        assert_eq!(
            pairs.next(),
            Some((Cow::Borrowed("action"), Cow::Borrowed("email")))
        );
        assert_eq!(
            pairs.next(),
            Some((Cow::Borrowed("response_type"), Cow::Borrowed("code")))
        );
        assert_eq!(
            pairs.next(),
            Some((
                Cow::Borrowed("entrypoint"),
                Cow::Borrowed("test_oauth_flow_url")
            ))
        );
        assert_eq!(
            pairs.next(),
            Some((Cow::Borrowed("client_id"), Cow::Borrowed("12345678")))
        );

        assert_eq!(
            pairs.next(),
            Some((Cow::Borrowed("scope"), Cow::Borrowed("profile")))
        );
        let state_param = pairs.next().unwrap();
        assert_eq!(state_param.0, Cow::Borrowed("state"));
        assert_eq!(state_param.1.len(), 22);
        assert_eq!(
            pairs.next(),
            Some((
                Cow::Borrowed("code_challenge_method"),
                Cow::Borrowed("S256")
            ))
        );
        let code_challenge_param = pairs.next().unwrap();
        assert_eq!(code_challenge_param.0, Cow::Borrowed("code_challenge"));
        assert_eq!(code_challenge_param.1.len(), 43);
        assert_eq!(
            pairs.next(),
            Some((Cow::Borrowed("access_type"), Cow::Borrowed("offline")))
        );
        let keys_jwk = pairs.next().unwrap();
        assert_eq!(keys_jwk.0, Cow::Borrowed("keys_jwk"));
        assert_eq!(keys_jwk.1.len(), 168);

        assert_eq!(
            pairs.next(),
            Some((
                Cow::Borrowed("redirect_uri"),
                Cow::Borrowed("https://foo.bar")
            ))
        );
    }

    #[test]
    fn test_force_auth_url() {
        nss::ensure_initialized();
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);
        let email = "test@example.com";
        fxa.add_cached_profile("123", email);
        let url = fxa
            .begin_oauth_flow(&["profile"], "test_force_auth_url")
            .unwrap();
        let url = Url::parse(&url).unwrap();
        assert_eq!(url.path(), "/oauth/force_auth");
        let mut pairs = url.query_pairs();
        assert_eq!(
            pairs.find(|e| e.0 == "email"),
            Some((Cow::Borrowed("email"), Cow::Borrowed(email),))
        );
    }

    #[test]
    fn test_webchannel_context_url() {
        nss::ensure_initialized();
        // FIXME: this test shouldn't make network requests.
        viaduct_reqwest::use_reqwest_backend();
        const SCOPES: &[&str] = &["https://identity.mozilla.com/apps/oldsync"];
        let config = Config::new(
            "https://accounts.firefox.com",
            "12345678",
            "urn:ietf:wg:oauth:2.0:oob:oauth-redirect-webchannel",
        );
        let mut fxa = FirefoxAccount::with_config(config);
        let url = fxa
            .begin_oauth_flow(SCOPES, "test_webchannel_context_url")
            .unwrap();
        let url = Url::parse(&url).unwrap();
        let query_params: HashMap<_, _> = url.query_pairs().into_owned().collect();
        let context = &query_params["context"];
        assert_eq!(context, "oauth_webchannel_v1");
        assert_eq!(query_params.get("redirect_uri"), None);
    }

    #[test]
    fn test_webchannel_pairing_context_url() {
        nss::ensure_initialized();
        const SCOPES: &[&str] = &["https://identity.mozilla.com/apps/oldsync"];
        const PAIRING_URL: &str = "https://accounts.firefox.com/pair#channel_id=658db7fe98b249a5897b884f98fb31b7&channel_key=1hIDzTj5oY2HDeSg_jA2DhcOcAn5Uqq0cAYlZRNUIo4";

        let config = Config::new(
            "https://accounts.firefox.com",
            "12345678",
            "urn:ietf:wg:oauth:2.0:oob:oauth-redirect-webchannel",
        );
        let mut fxa = FirefoxAccount::with_config(config);
        let url = fxa
            .begin_pairing_flow(PAIRING_URL, SCOPES, "test_webchannel_pairing_context_url")
            .unwrap();
        let url = Url::parse(&url).unwrap();
        let query_params: HashMap<_, _> = url.query_pairs().into_owned().collect();
        let context = &query_params["context"];
        assert_eq!(context, "oauth_webchannel_v1");
        assert_eq!(query_params.get("redirect_uri"), None);
    }

    #[test]
    fn test_pairing_flow_url() {
        nss::ensure_initialized();
        const SCOPES: &[&str] = &["https://identity.mozilla.com/apps/oldsync"];
        const PAIRING_URL: &str = "https://accounts.firefox.com/pair#channel_id=658db7fe98b249a5897b884f98fb31b7&channel_key=1hIDzTj5oY2HDeSg_jA2DhcOcAn5Uqq0cAYlZRNUIo4";
        const EXPECTED_URL: &str = "https://accounts.firefox.com/pair/supp?client_id=12345678&redirect_uri=https%3A%2F%2Ffoo.bar&scope=https%3A%2F%2Fidentity.mozilla.com%2Fapps%2Foldsync&state=SmbAA_9EA5v1R2bgIPeWWw&code_challenge_method=S256&code_challenge=ZgHLPPJ8XYbXpo7VIb7wFw0yXlTa6MUOVfGiADt0JSM&access_type=offline&keys_jwk=eyJjcnYiOiJQLTI1NiIsImt0eSI6IkVDIiwieCI6Ing5LUltQjJveDM0LTV6c1VmbW5sNEp0Ti14elV2eFZlZXJHTFRXRV9BT0kiLCJ5IjoiNXBKbTB3WGQ4YXdHcm0zREl4T1pWMl9qdl9tZEx1TWlMb1RkZ1RucWJDZyJ9#channel_id=658db7fe98b249a5897b884f98fb31b7&channel_key=1hIDzTj5oY2HDeSg_jA2DhcOcAn5Uqq0cAYlZRNUIo4";

        let config = Config::new(
            "https://accounts.firefox.com",
            "12345678",
            "https://foo.bar",
        );

        let mut fxa = FirefoxAccount::with_config(config);
        let url = fxa
            .begin_pairing_flow(PAIRING_URL, SCOPES, "test_pairing_flow_url")
            .unwrap();
        let flow_url = Url::parse(&url).unwrap();
        let expected_parsed_url = Url::parse(EXPECTED_URL).unwrap();

        assert_eq!(flow_url.host_str(), Some("accounts.firefox.com"));
        assert_eq!(flow_url.path(), "/pair/supp");
        assert_eq!(flow_url.fragment(), expected_parsed_url.fragment());

        let mut pairs = flow_url.query_pairs();
        assert_eq!(pairs.count(), 9);
        assert_eq!(
            pairs.next(),
            Some((
                Cow::Borrowed("entrypoint"),
                Cow::Borrowed("test_pairing_flow_url")
            ))
        );
        assert_eq!(
            pairs.next(),
            Some((Cow::Borrowed("client_id"), Cow::Borrowed("12345678")))
        );
        assert_eq!(
            pairs.next(),
            Some((
                Cow::Borrowed("scope"),
                Cow::Borrowed("https://identity.mozilla.com/apps/oldsync")
            ))
        );

        let state_param = pairs.next().unwrap();
        assert_eq!(state_param.0, Cow::Borrowed("state"));
        assert_eq!(state_param.1.len(), 22);
        assert_eq!(
            pairs.next(),
            Some((
                Cow::Borrowed("code_challenge_method"),
                Cow::Borrowed("S256")
            ))
        );
        let code_challenge_param = pairs.next().unwrap();
        assert_eq!(code_challenge_param.0, Cow::Borrowed("code_challenge"));
        assert_eq!(code_challenge_param.1.len(), 43);
        assert_eq!(
            pairs.next(),
            Some((Cow::Borrowed("access_type"), Cow::Borrowed("offline")))
        );
        let keys_jwk = pairs.next().unwrap();
        assert_eq!(keys_jwk.0, Cow::Borrowed("keys_jwk"));
        assert_eq!(keys_jwk.1.len(), 168);

        assert_eq!(
            pairs.next(),
            Some((
                Cow::Borrowed("redirect_uri"),
                Cow::Borrowed("https://foo.bar")
            ))
        );
    }

    #[test]
    fn test_pairing_flow_origin_mismatch() {
        nss::ensure_initialized();
        static PAIRING_URL: &str = "https://bad.origin.com/pair#channel_id=foo&channel_key=bar";
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);
        let url = fxa.begin_pairing_flow(
            PAIRING_URL,
            &["https://identity.mozilla.com/apps/oldsync"],
            "test_pairiong_flow_origin_mismatch",
        );

        assert!(url.is_err());

        match url {
            Ok(_) => {
                panic!("should have error");
            }
            Err(err) => match err {
                Error::OriginMismatch { .. } => {}
                _ => panic!("error not OriginMismatch"),
            },
        }
    }

    #[test]
    fn test_check_authorization_status() {
        nss::ensure_initialized();
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);

        let refresh_token_scopes = std::collections::HashSet::new();
        fxa.state.force_refresh_token(RefreshToken {
            token: "refresh_token".to_owned(),
            scopes: refresh_token_scopes,
        });

        let mut client = MockFxAClient::new();
        client
            .expect_check_refresh_token_status()
            .with(always(), eq("refresh_token"))
            .times(1)
            .returning(|_, _| Ok(IntrospectResponse { active: true }));
        fxa.set_client(Arc::new(client));

        let auth_status = fxa.check_authorization_status().unwrap();
        assert!(auth_status.active);
    }

    #[test]
    fn test_check_authorization_status_circuit_breaker() {
        nss::ensure_initialized();
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);

        let refresh_token_scopes = std::collections::HashSet::new();
        fxa.state.force_refresh_token(RefreshToken {
            token: "refresh_token".to_owned(),
            scopes: refresh_token_scopes,
        });

        let mut client = MockFxAClient::new();
        // This copy-pasta (equivalent to `.returns(..).times(5)`) is there
        // because `Error` is not cloneable :/
        client
            .expect_check_refresh_token_status()
            .with(always(), eq("refresh_token"))
            .returning(|_, _| Ok(IntrospectResponse { active: true }));
        client
            .expect_check_refresh_token_status()
            .with(always(), eq("refresh_token"))
            .returning(|_, _| Ok(IntrospectResponse { active: true }));
        client
            .expect_check_refresh_token_status()
            .with(always(), eq("refresh_token"))
            .returning(|_, _| Ok(IntrospectResponse { active: true }));
        client
            .expect_check_refresh_token_status()
            .with(always(), eq("refresh_token"))
            .returning(|_, _| Ok(IntrospectResponse { active: true }));
        client
            .expect_check_refresh_token_status()
            .with(always(), eq("refresh_token"))
            .returning(|_, _| Ok(IntrospectResponse { active: true }));
        //mockall expects calls to be processed in the order they are registered. So, no need for to use a method like expect_check_refresh_token_status_calls_in_order()
        fxa.set_client(Arc::new(client));

        for _ in 0..5 {
            assert!(fxa.check_authorization_status().is_ok());
        }
        match fxa.check_authorization_status() {
            Ok(_) => unreachable!("should not happen"),
            Err(err) => assert!(matches!(err, Error::AuthCircuitBreakerError)),
        }
    }

    use crate::internal::scopes::{self, OLD_SYNC};

    #[test]
    fn test_auth_code_pair_valid_not_allowed_scope() {
        nss::ensure_initialized();
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);
        fxa.set_session_token("session");
        let mut client = MockFxAClient::new();
        let not_allowed_scope = "https://identity.mozilla.com/apps/lockbox";
        let expected_scopes = scopes::OLD_SYNC
            .chars()
            .chain(std::iter::once(' '))
            .chain(not_allowed_scope.chars())
            .collect::<String>();
        client
            .expect_get_scoped_key_data()
            .with(always(), eq("session"), eq("12345678"), eq(expected_scopes))
            .times(1)
            .returning(|_, _, _, _| {
                Err(Error::RemoteError {
                    code: 400,
                    errno: 163,
                    error: "Invalid Scopes".to_string(),
                    message: "Not allowed to request scopes".to_string(),
                    info: "fyi, there was a server error".to_string(),
                })
            });
        fxa.set_client(Arc::new(client));
        let auth_params = AuthorizationParameters {
            client_id: "12345678".to_string(),
            scope: vec![scopes::OLD_SYNC.to_string(), not_allowed_scope.to_string()],
            state: "somestate".to_string(),
            access_type: "offline".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            keys_jwk: None,
        };
        let res = fxa.authorize_code_using_session_token(auth_params);
        assert!(res.is_err());
        let err = res.unwrap_err();
        if let Error::RemoteError {
            code,
            errno,
            error: _,
            message: _,
            info: _,
        } = err
        {
            assert_eq!(code, 400);
            assert_eq!(errno, 163); // Requested scopes not allowed
        } else {
            panic!("Should return an error from the server specifying that the requested scopes are not allowed");
        }
    }

    #[test]
    fn test_auth_code_pair_invalid_scope_not_allowed() {
        nss::ensure_initialized();
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);
        fxa.set_session_token("session");
        let mut client = MockFxAClient::new();
        let invalid_scope = "IamAnInvalidScope";
        let expected_scopes = scopes::OLD_SYNC
            .chars()
            .chain(std::iter::once(' '))
            .chain(invalid_scope.chars())
            .collect::<String>();
        client
            .expect_get_scoped_key_data()
            .with(always(), eq("session"), eq("12345678"), eq(expected_scopes))
            .times(1)
            .returning(|_, _, _, _| {
                let mut server_ret = HashMap::new();
                server_ret.insert(
                    scopes::OLD_SYNC.to_string(),
                    ScopedKeyDataResponse {
                        key_rotation_secret: "IamASecret".to_string(),
                        key_rotation_timestamp: 100,
                        identifier: "".to_string(),
                    },
                );
                Ok(server_ret)
            });
        fxa.set_client(Arc::new(client));

        let auth_params = AuthorizationParameters {
            client_id: "12345678".to_string(),
            scope: vec![scopes::OLD_SYNC.to_string(), invalid_scope.to_string()],
            state: "somestate".to_string(),
            access_type: "offline".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            keys_jwk: None,
        };
        let res = fxa.authorize_code_using_session_token(auth_params);
        assert!(res.is_err());
        let err = res.unwrap_err();
        if let Error::ScopeNotAllowed(client_id, scope) = err {
            assert_eq!(client_id, "12345678");
            assert_eq!(scope, "IamAnInvalidScope");
        } else {
            panic!("Should return an error that specifies the scope that is not allowed");
        }
    }

    #[test]
    fn test_auth_code_pair_scope_not_in_state() {
        nss::ensure_initialized();
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);
        fxa.set_session_token("session");
        let mut client = MockFxAClient::new();
        client
            .expect_get_scoped_key_data()
            .with(
                always(),
                eq("session"),
                eq("12345678"),
                eq(scopes::OLD_SYNC),
            )
            .times(1)
            .returning(|_, _, _, _| {
                let mut server_ret = HashMap::new();
                server_ret.insert(
                    scopes::OLD_SYNC.to_string(),
                    ScopedKeyDataResponse {
                        key_rotation_secret: "IamASecret".to_string(),
                        key_rotation_timestamp: 100,
                        identifier: "".to_string(),
                    },
                );
                Ok(server_ret)
            });
        fxa.set_client(Arc::new(client));
        let auth_params = AuthorizationParameters {
            client_id: "12345678".to_string(),
            scope: vec![scopes::OLD_SYNC.to_string()],
            state: "somestate".to_string(),
            access_type: "offline".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            keys_jwk: Some("IAmAVerySecretKeysJWkInBase64".to_string()),
        };
        let res = fxa.authorize_code_using_session_token(auth_params);
        assert!(res.is_err());
        let err = res.unwrap_err();
        if let Error::NoScopedKey(scope) = err {
            assert_eq!(scope, scopes::OLD_SYNC.to_string());
        } else {
            panic!("Should return an error that specifies the scope that is not in the state");
        }
    }

    #[test]
    fn test_set_user_data_sets_session_token() {
        nss::ensure_initialized();
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);
        let user_data = UserData {
            session_token: String::from("mock_session_token"),
            uid: String::from("mock_uid_unused"),
            email: String::from("mock_email_usued"),
            verified: true,
        };
        fxa.set_user_data(user_data);
        assert_eq!(fxa.get_session_token().unwrap(), "mock_session_token");
    }

    #[test]
    fn test_oauth_request_sent_with_session_when_available() {
        nss::ensure_initialized();
        let config = Config::new(
            "https://accounts.firefox.com",
            "12345678",
            "https://foo.bar",
        );
        let mut fxa = FirefoxAccount::with_config(config);
        let url = fxa
            .begin_oauth_flow(&[OLD_SYNC, "profile"], "test_entrypoint")
            .unwrap();
        let url = Url::parse(&url).unwrap();
        let state = url.query_pairs().find(|(name, _)| name == "state").unwrap();
        let user_data = UserData {
            session_token: String::from("mock_session_token"),
            uid: String::from("mock_uid_unused"),
            email: String::from("mock_email_usued"),
            verified: true,
        };
        let mut client = MockFxAClient::new();

        client
            .expect_create_refresh_token_using_authorization_code()
            .withf(|_, session_token, code, _| {
                matches!(session_token, Some("mock_session_token")) && code == "mock_code"
            })
            .times(1)
            .returning(|_, _, _, _| {
                Ok(OAuthTokenResponse {
                    keys_jwe: None,
                    refresh_token: Some("refresh_token".to_string()),
                    session_token: None,
                    expires_in: 1,
                    scope: "profile".to_string(),
                    access_token: "access_token".to_string(),
                })
            });
        client
            .expect_destroy_access_token()
            .with(always(), always())
            .times(1)
            .returning(|_, _| Ok(()));
        fxa.set_client(Arc::new(client));

        fxa.set_user_data(user_data);

        fxa.complete_oauth_flow("mock_code", state.1.as_ref())
            .unwrap();
    }
}

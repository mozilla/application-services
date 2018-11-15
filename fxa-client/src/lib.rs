/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate base64;
extern crate byteorder;
extern crate failure;
#[macro_use]
extern crate failure_derive;
#[cfg(feature = "browserid")]
extern crate hawk;
extern crate hex;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[cfg(feature = "browserid")]
extern crate openssl;
extern crate regex;
extern crate reqwest;
extern crate ring;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate untrusted;
extern crate url;
#[cfg(feature = "ffi")]
#[macro_use]
extern crate ffi_support;

use std::collections::HashMap;
use std::collections::HashSet;
use std::iter::FromIterator;
#[cfg(feature = "browserid")]
use std::mem;
use std::panic::RefUnwindSafe;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "browserid")]
use self::login_sm::LoginState::*;
#[cfg(feature = "browserid")]
use self::login_sm::*;
use errors::*;
#[cfg(feature = "browserid")]
use http_client::browser_id::jwt_utils;
use http_client::{Client, ProfileResponse};
use ring::digest;
use ring::rand::{SecureRandom, SystemRandom};
use scoped_keys::ScopedKeysFlow;
use url::Url;
use util::now;

mod config;
pub mod errors;
mod http_client;
#[cfg(feature = "browserid")]
mod login_sm;
mod state_persistence;
mod scoped_keys;
mod util;
#[cfg(feature = "ffi")]
pub mod ffi;

pub use config::Config;
pub use http_client::ProfileResponse as Profile;

// If a cached token has less than `OAUTH_MIN_TIME_LEFT` seconds left to live,
// it will be considered already expired.
const OAUTH_MIN_TIME_LEFT: u64 = 60;
// A cached profile response is considered fresh for `PROFILE_FRESHNESS_THRESHOLD` ms.
const PROFILE_FRESHNESS_THRESHOLD: u64 = 120000; // 2 minutes

lazy_static! {
    static ref RNG: SystemRandom = SystemRandom::new();
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StateV2 {
    config: Config,
    #[cfg(feature = "browserid")]
    login_state: LoginState,
    refresh_token: Option<RefreshToken>,
    scoped_keys: HashMap<String, serde_json::Value>,
}

#[cfg(feature = "browserid")]
#[derive(Deserialize)]
pub struct WebChannelResponse {
    uid: String,
    email: String,
    verified: bool,
    #[serde(rename = "sessionToken")]
    session_token: String,
    #[serde(rename = "keyFetchToken")]
    key_fetch_token: String,
    #[serde(rename = "unwrapBKey")]
    unwrap_kb: String,
}

#[cfg(feature = "browserid")]
impl WebChannelResponse {
    pub fn from_json(json: &str) -> Result<WebChannelResponse> {
        serde_json::from_str(json).map_err(|e| e.into())
    }
}

struct CachedResponse<T> {
    response: T,
    cached_at: u64,
    etag: String,
}

pub struct FirefoxAccount {
    state: StateV2,
    access_token_cache: HashMap<String, AccessTokenInfo>,
    flow_store: HashMap<String, OAuthFlow>,
    persist_callback: Option<PersistCallback>,
    profile_cache: Option<CachedResponse<ProfileResponse>>,
}

pub struct SyncKeys(pub String, pub String);

pub struct PersistCallback {
    callback_fn: Box<Fn(&str) + Send + RefUnwindSafe>,
}

impl PersistCallback {
    pub fn new<F>(callback_fn: F) -> PersistCallback
    where
        F: Fn(&str) + 'static + Send + RefUnwindSafe,
    {
        PersistCallback {
            callback_fn: Box::new(callback_fn),
        }
    }

    pub fn call(&self, json: &str) {
        (*self.callback_fn)(json);
    }
}

impl FirefoxAccount {
    fn from_state(state: StateV2) -> FirefoxAccount {
        FirefoxAccount {
            state,
            access_token_cache: HashMap::new(),
            flow_store: HashMap::new(),
            persist_callback: None,
            profile_cache: None,
        }
    }

    pub fn new(config: Config) -> FirefoxAccount {
        FirefoxAccount::from_state(StateV2 {
            config,
            #[cfg(feature = "browserid")]
            login_state: Unknown,
            refresh_token: None,
            scoped_keys: HashMap::new(),
        })
    }

    // Initialize state from Firefox Accounts credentials obtained using the
    // web flow.
    #[cfg(feature = "browserid")]
    pub fn from_credentials(
        config: Config,
        credentials: WebChannelResponse,
    ) -> Result<FirefoxAccount> {
        let session_token = hex::decode(credentials.session_token)?;
        let key_fetch_token = hex::decode(credentials.key_fetch_token)?;
        let unwrap_kb = hex::decode(credentials.unwrap_kb)?;
        let login_state_data = ReadyForKeysState::new(
            credentials.uid,
            credentials.email,
            session_token,
            key_fetch_token,
            unwrap_kb,
        );
        let login_state = if credentials.verified {
            EngagedAfterVerified(login_state_data)
        } else {
            EngagedBeforeVerified(login_state_data)
        };

        Ok(FirefoxAccount::from_state(StateV2 {
            config,
            login_state,
            refresh_token: None,
            scoped_keys: HashMap::new(),
        }))
    }

    pub fn from_json(data: &str) -> Result<FirefoxAccount> {
        let state = state_persistence::state_from_json(data)?;
        Ok(FirefoxAccount::from_state(state))
    }

    pub fn to_json(&self) -> Result<String> {
        state_persistence::state_to_json(&self.state)
    }

    #[cfg(feature = "browserid")]
    fn to_married(&mut self) -> Option<&MarriedState> {
        self.advance();
        match self.state.login_state {
            Married(ref married) => Some(married),
            _ => None,
        }
    }

    #[cfg(feature = "browserid")]
    pub fn advance(&mut self) {
        let client = Client::new(&self.state.config);
        let state_machine = LoginStateMachine::new(client);
        let state = mem::replace(&mut self.state.login_state, Unknown);
        self.state.login_state = state_machine.advance(state);
    }

    pub fn get_access_token(&mut self, scope: &str) -> Result<AccessTokenInfo> {
        if scope.contains(" ") {
            return Err(ErrorKind::MultipleScopesRequested.into())
        }
        if let Some(oauth_info) = self.access_token_cache.get(scope) {
            if oauth_info.expires_at > util::now_secs() + OAUTH_MIN_TIME_LEFT {
                return Ok(oauth_info.clone());
            }
        }
        // This is a bit awkward, borrow checker weirdness.
        let resp;
        {
            if let Some(ref refresh_token) = self.state.refresh_token {
                if refresh_token.scopes.contains(scope) {
                    let client = Client::new(&self.state.config);
                    resp = client.oauth_token_with_refresh_token(
                        &refresh_token.token,
                        &[scope],
                    )?;
                } else {
                    return Err(ErrorKind::NoCachedToken(scope.to_string()).into())
                }
            } else {
                #[cfg(feature = "browserid")]
                {
                    if let Some(session_token) =
                        FirefoxAccount::session_token_from_state(&self.state.login_state)
                    {
                        let client = Client::new(&self.state.config);
                        resp = client.oauth_token_with_session_token(
                            session_token,
                            &[scope],
                        )?;
                    } else {
                        return Err(ErrorKind::NoCachedToken(scope.to_string()).into())
                    }
                }
                #[cfg(not(feature = "browserid"))]
                {
                    return Err(ErrorKind::NoCachedToken(scope.to_string()).into())
                }
            }
        }
        let key = match self.state.scoped_keys.get(scope) {
            Some(k) => match serde_json::to_string(k) {
                Ok(k) => Some(k),
                _ => None,
            },
            None => None,
        };
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ErrorKind::IllegalState("Current date before Unix Epoch.".to_string()))?;
        let expires_at = since_epoch.as_secs() + resp.expires_in;
        let token_info = AccessTokenInfo {
            scope: resp.scope,
            token: resp.access_token,
            key,
            expires_at,
        };
        self.access_token_cache.insert(scope.to_string(), token_info.clone());
        Ok(token_info)
    }

    pub fn begin_pairing_flow(&mut self, pairing_url: &str, scopes: &[&str]) -> Result<String> {
        let mut url = self.state.config.content_url_path("/pair/supp")?;
        let pairing_url = Url::parse(pairing_url)?;
        if url.host_str() != pairing_url.host_str() {
            return Err(ErrorKind::OriginMismatch.into());
        }
        url.set_fragment(pairing_url.fragment());
        self.oauth_flow(url, scopes, true)
    }

    pub fn begin_oauth_flow(&mut self, scopes: &[&str], wants_keys: bool) -> Result<String> {
        let mut url = self.state.config.authorization_endpoint()?;
        url.query_pairs_mut()
            .append_pair("action", "email")
            .append_pair("response_type", "code");
        let scopes: Vec<String> = match self.state.refresh_token {
            Some(ref refresh_token) => {
                // Union of the already held scopes and the one requested.
                let mut all_scopes: Vec<String> = vec![];
                all_scopes.extend(scopes.iter().map(|s| s.to_string()));
                let existing_scopes = refresh_token.scopes.clone();
                all_scopes.extend(existing_scopes);
                HashSet::<String>::from_iter(all_scopes).into_iter().collect()
            },
            None => scopes.iter().map(|s| s.to_string()).collect(),
        };
        let scopes: Vec<&str> = scopes.iter().map(<_>::as_ref).collect();
        self.oauth_flow(url, &scopes, wants_keys)
    }

    fn oauth_flow(&mut self, mut url: Url, scopes: &[&str], wants_keys: bool) -> Result<String> {
        let state = FirefoxAccount::random_base64_url_string(16)?;
        let code_verifier = FirefoxAccount::random_base64_url_string(43)?;
        let code_challenge = digest::digest(&digest::SHA256, &code_verifier.as_bytes());
        let code_challenge = base64::encode_config(&code_challenge, base64::URL_SAFE_NO_PAD);
        url.query_pairs_mut()
            .append_pair("client_id", &self.state.config.client_id)
            .append_pair("redirect_uri", &self.state.config.redirect_uri)
            .append_pair("scope", &scopes.join(" "))
            .append_pair("state", &state)
            .append_pair("code_challenge_method", "S256")
            .append_pair("code_challenge", &code_challenge)
            .append_pair("access_type", "offline");
        let scoped_keys_flow = match wants_keys {
            true => {
                let flow = ScopedKeysFlow::with_random_key(&*RNG)?;
                let jwk_json = flow.generate_keys_jwk()?;
                let keys_jwk = base64::encode_config(&jwk_json, base64::URL_SAFE_NO_PAD);
                url.query_pairs_mut().append_pair("keys_jwk", &keys_jwk);
                Some(flow)
            }
            false => None,
        };
        self.flow_store.insert(
            state.clone(), // Since state is supposed to be unique, we use it to key our flows.
            OAuthFlow {
                scoped_keys_flow,
                code_verifier,
            },
        );
        Ok(url.to_string())
    }

    pub fn complete_oauth_flow(&mut self, code: &str, state: &str) -> Result<()> {
        let resp;
        // Needs non-lexical borrow checking.
        {
            let flow = match self.flow_store.get(state) {
                Some(flow) => flow,
                None => return Err(ErrorKind::UnknownOAuthState.into()),
            };
            let client = Client::new(&self.state.config);
            resp = client.oauth_token_with_code(&code, &flow.code_verifier)?;
        }
        let oauth_flow = match self.flow_store.remove(state) {
            Some(oauth_flow) => oauth_flow,
            None => return Err(ErrorKind::UnknownOAuthState.into()),
        };
        // This assumes that if the server returns keys_jwe, the jwk argument is Some.
        match resp.keys_jwe {
            Some(ref jwe) => {
                let scoped_keys_flow = match oauth_flow.scoped_keys_flow {
                    Some(flow) => flow,
                    None => return Err(ErrorKind::IllegalState("Got a JWE with have no JWK.".to_string()).into())
                };
                let decrypted_keys = scoped_keys_flow.decrypt_keys_jwe(jwe)?;
                let scoped_keys: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&decrypted_keys)?;
                for (scope, key) in scoped_keys {
                    self.state.scoped_keys.insert(scope.clone(), key.clone());
                }
            }
            None => {
                if oauth_flow.scoped_keys_flow.is_some() {
                    error!("Expected to get keys back alongside the token but the server didn't send them.");
                    return Err(ErrorKind::TokenWithoutKeys.into());
                }
            }
        };
        let client = Client::new(&self.state.config);
        // We are only interested in the refresh token at this time because we
        // don't want to return an over-scoped access token.
        // Let's be good citizens and destroy this access token.
        if let Err(err) = client.destroy_oauth_token(&resp.access_token) {
            warn!("Access token destruction failure: {:?}", err);
        }
        let refresh_token = match resp.refresh_token {
            Some(ref refresh_token) => refresh_token.clone(),
            None => return Err(ErrorKind::RefreshTokenNotPresent.into())
        };
        // In order to keep 1 and only 1 refresh token alive per client instance,
        // we also destroy the existing refresh token.
        if let Some(ref old_refresh_token) = self.state.refresh_token {
            if let Err(err) = client.destroy_oauth_token(&old_refresh_token.token) {
                warn!("Refresh token destruction failure: {:?}", err);
            }
        }
        self.state.refresh_token = Some(RefreshToken {
            token: refresh_token,
            scopes: HashSet::from_iter(resp.scope.split(' ').map(|s| s.to_string())),
        });
        self.maybe_call_persist_callback();
        Ok(())
    }

    fn random_base64_url_string(len: usize) -> Result<String> {
        let mut out = vec![0u8; len];
        RNG.fill(&mut out).map_err(|_| ErrorKind::RngFailure)?;
        Ok(base64::encode_config(&out, base64::URL_SAFE_NO_PAD))
    }

    #[cfg(feature = "browserid")]
    fn session_token_from_state(state: &LoginState) -> Option<&[u8]> {
        match state {
            &Separated(_) | Unknown => None,
            // Despite all these states implementing the same trait we can't treat
            // them in a single arm, so this will do for now :/
            &EngagedBeforeVerified(ref state) | &EngagedAfterVerified(ref state) => {
                Some(state.session_token())
            }
            &CohabitingBeforeKeyPair(ref state) => Some(state.session_token()),
            &CohabitingAfterKeyPair(ref state) => Some(state.session_token()),
            &Married(ref state) => Some(state.session_token()),
        }
    }

    #[cfg(feature = "browserid")]
    pub fn generate_assertion(&mut self, audience: &str) -> Result<String> {
        let married = match self.to_married() {
            Some(married) => married,
            None => return Err(ErrorKind::NotMarried.into()),
        };
        let key_pair = married.key_pair();
        let certificate = married.certificate();
        Ok(jwt_utils::create_assertion(
            key_pair,
            &certificate,
            audience,
        )?)
    }

    pub fn get_profile(&mut self, ignore_cache: bool) -> Result<ProfileResponse> {
        let profile_access_token = self.get_access_token("profile")?.token;
        let mut etag = None;
        if let Some(ref cached_profile) = self.profile_cache {
            if !ignore_cache && now() < cached_profile.cached_at + PROFILE_FRESHNESS_THRESHOLD {
                return Ok(cached_profile.response.clone());
            }
            etag = Some(cached_profile.etag.clone());
        }
        let client = Client::new(&self.state.config);
        match client.profile(&profile_access_token, etag)? {
            Some(response_and_etag) => {
                if let Some(etag) = response_and_etag.etag {
                    self.profile_cache = Some(CachedResponse {
                        response: response_and_etag.response.clone(),
                        cached_at: now(),
                        etag,
                    });
                }
                Ok(response_and_etag.response)
            }
            None => match self.profile_cache {
                Some(ref cached_profile) => Ok(cached_profile.response.clone()),
                None => {
                    error!("Insane state! We got a 304 without having a cached response.");
                    Err(ErrorKind::UnrecoverableServerError.into())
                }
            },
        }
    }

    #[cfg(feature = "browserid")]
    pub fn get_sync_keys(&mut self) -> Result<SyncKeys> {
        let married = match self.to_married() {
            Some(married) => married,
            None => return Err(ErrorKind::NotMarried.into()),
        };
        let sync_key = hex::encode(married.sync_key());
        Ok(SyncKeys(sync_key, married.xcs().to_string()))
    }

    pub fn get_token_server_endpoint_url(&self) -> Result<Url> {
        self.state.config.token_server_endpoint_url()
    }

    pub fn get_connection_success_url(&self) -> Result<Url> {
        let mut url = self.state.config.content_url_path("connect_another_device")?;
        url.query_pairs_mut().append_pair("showSuccessMessage", "true");
        Ok(url)
    }

    pub fn handle_push_message(&self) {
        panic!("Not implemented yet!")
    }

    pub fn register_device(&self) {
        panic!("Not implemented yet!")
    }

    pub fn get_devices_list(&self) {
        panic!("Not implemented yet!")
    }

    pub fn send_message(&self) {
        panic!("Not implemented yet!")
    }

    pub fn retrieve_messages(&self) {
        panic!("Not implemented yet!")
    }

    pub fn register_persist_callback(&mut self, persist_callback: PersistCallback) {
        self.persist_callback = Some(persist_callback);
    }

    pub fn unregister_persist_callback(&mut self) {
        self.persist_callback = None;
    }

    fn maybe_call_persist_callback(&self) {
        if let Some(ref cb) = self.persist_callback {
            let json = match self.to_json() {
                Ok(json) => json,
                Err(_) => {
                    error!("Error with to_json in persist_callback");
                    return;
                }
            };
            cb.call(&json);
        }
    }

    #[cfg(feature = "browserid")]
    pub fn sign_out(mut self) {
        let client = Client::new(&self.state.config);
        client.sign_out();
        self.state.login_state = self.state.login_state.to_separated();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefreshToken {
    pub token: String,
    pub scopes: HashSet<String>,
}

pub struct OAuthFlow {
    pub scoped_keys_flow: Option<ScopedKeysFlow>,
    pub code_verifier: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessTokenInfo {
    pub scope: String,
    pub token: String,
    pub key: Option<String>,
    pub expires_at: u64, // seconds since epoch
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn test_fxa_is_send() {
        fn is_send<T: Send>() {}
        is_send::<FirefoxAccount>();
    }

    #[test]
    fn test_serialize_deserialize() {
        let fxa1 = FirefoxAccount::new(Config::stable_dev("12345678", "https://foo.bar").unwrap());
        let fxa1_json = fxa1.to_json().unwrap();
        drop(fxa1);
        let fxa2 = FirefoxAccount::from_json(&fxa1_json).unwrap();
        let fxa2_json = fxa2.to_json().unwrap();
        assert_eq!(fxa1_json, fxa2_json);
    }

    #[test]
    fn test_oauth_flow_url() {
        let mut fxa = FirefoxAccount::new(Config::release("12345678", "https://foo.bar").unwrap());
        let url = fxa.begin_oauth_flow(&["profile"], false).unwrap();
        let flow_url = Url::parse(&url).unwrap();

        assert_eq!(flow_url.host_str(), Some("accounts.firefox.com"));
        assert_eq!(flow_url.path(), "/authorization");

        let mut pairs = flow_url.query_pairs();
        assert_eq!(pairs.count(), 9);
        assert_eq!(pairs.next(), Some((Cow::Borrowed("action"), Cow::Borrowed("email"))));
        assert_eq!(pairs.next(), Some((Cow::Borrowed("response_type"), Cow::Borrowed("code"))));
        assert_eq!(pairs.next(), Some((Cow::Borrowed("client_id"), Cow::Borrowed("12345678"))));
        assert_eq!(pairs.next(), Some((Cow::Borrowed("redirect_uri"), Cow::Borrowed("https://foo.bar"))));
        assert_eq!(pairs.next(), Some((Cow::Borrowed("scope"), Cow::Borrowed("profile"))));
        let state_param = pairs.next().unwrap();
        assert_eq!(state_param.0, Cow::Borrowed("state"));
        assert_eq!(state_param.1.len(), 22);
        assert_eq!(pairs.next(), Some((Cow::Borrowed("code_challenge_method"), Cow::Borrowed("S256"))));
        let code_challenge_param = pairs.next().unwrap();
        assert_eq!(code_challenge_param.0, Cow::Borrowed("code_challenge"));
        assert_eq!(code_challenge_param.1.len(), 43);
        assert_eq!(pairs.next(), Some((Cow::Borrowed("access_type"), Cow::Borrowed("offline"))));
    }

    #[test]
    fn test_oauth_flow_url_with_keys() {
        let mut fxa = FirefoxAccount::new(Config::release("12345678", "https://foo.bar").unwrap());
        let url = fxa.begin_oauth_flow(&["profile"], true).unwrap();
        let flow_url = Url::parse(&url).unwrap();

        assert_eq!(flow_url.host_str(), Some("accounts.firefox.com"));
        assert_eq!(flow_url.path(), "/authorization");

        let mut pairs = flow_url.query_pairs();
        assert_eq!(pairs.count(), 10);
        assert_eq!(pairs.next(), Some((Cow::Borrowed("action"), Cow::Borrowed("email"))));
        assert_eq!(pairs.next(), Some((Cow::Borrowed("response_type"), Cow::Borrowed("code"))));
        assert_eq!(pairs.next(), Some((Cow::Borrowed("client_id"), Cow::Borrowed("12345678"))));
        assert_eq!(pairs.next(), Some((Cow::Borrowed("redirect_uri"), Cow::Borrowed("https://foo.bar"))));
        assert_eq!(pairs.next(), Some((Cow::Borrowed("scope"), Cow::Borrowed("profile"))));
        let state_param = pairs.next().unwrap();
        assert_eq!(state_param.0, Cow::Borrowed("state"));
        assert_eq!(state_param.1.len(), 22);
        assert_eq!(pairs.next(), Some((Cow::Borrowed("code_challenge_method"), Cow::Borrowed("S256"))));
        let code_challenge_param = pairs.next().unwrap();
        assert_eq!(code_challenge_param.0, Cow::Borrowed("code_challenge"));
        assert_eq!(code_challenge_param.1.len(), 43);
        assert_eq!(pairs.next(), Some((Cow::Borrowed("access_type"), Cow::Borrowed("offline"))));
        let keys_jwk = pairs.next().unwrap();
        assert_eq!(keys_jwk.0, Cow::Borrowed("keys_jwk"));
        assert_eq!(keys_jwk.1.len(), 168);
    }

    #[test]
    fn test_pairing_flow_url() {
        static SCOPES: &'static [&'static str] = &["https://identity.mozilla.com/apps/oldsync"];
        static PAIRING_URL: &'static str = "https://accounts.firefox.com/pair#channel_id=658db7fe98b249a5897b884f98fb31b7&channel_key=1hIDzTj5oY2HDeSg_jA2DhcOcAn5Uqq0cAYlZRNUIo4";
        static EXPECTED_URL: &'static str = "https://accounts.firefox.com/pair/supp?client_id=12345678&redirect_uri=https%3A%2F%2Ffoo.bar&scope=https%3A%2F%2Fidentity.mozilla.com%2Fapps%2Foldsync&state=SmbAA_9EA5v1R2bgIPeWWw&code_challenge_method=S256&code_challenge=ZgHLPPJ8XYbXpo7VIb7wFw0yXlTa6MUOVfGiADt0JSM&access_type=offline&keys_jwk=eyJjcnYiOiJQLTI1NiIsImt0eSI6IkVDIiwieCI6Ing5LUltQjJveDM0LTV6c1VmbW5sNEp0Ti14elV2eFZlZXJHTFRXRV9BT0kiLCJ5IjoiNXBKbTB3WGQ4YXdHcm0zREl4T1pWMl9qdl9tZEx1TWlMb1RkZ1RucWJDZyJ9#channel_id=658db7fe98b249a5897b884f98fb31b7&channel_key=1hIDzTj5oY2HDeSg_jA2DhcOcAn5Uqq0cAYlZRNUIo4";

        let mut fxa = FirefoxAccount::new(Config::release("12345678", "https://foo.bar").unwrap());
        let url = fxa.begin_pairing_flow(&PAIRING_URL, &SCOPES).unwrap();
        let flow_url = Url::parse(&url).unwrap();
        let expected_parsed_url = Url::parse(EXPECTED_URL).unwrap();

        assert_eq!(flow_url.host_str(), Some("accounts.firefox.com"));
        assert_eq!(flow_url.path(), "/pair/supp");
        assert_eq!(flow_url.fragment(), expected_parsed_url.fragment());

        let mut pairs = flow_url.query_pairs();
        assert_eq!(pairs.count(), 8);
        assert_eq!(pairs.next(), Some((Cow::Borrowed("client_id"), Cow::Borrowed("12345678"))));
        assert_eq!(pairs.next(), Some((Cow::Borrowed("redirect_uri"), Cow::Borrowed("https://foo.bar"))));
        assert_eq!(pairs.next(), Some((Cow::Borrowed("scope"), Cow::Borrowed("https://identity.mozilla.com/apps/oldsync"))));
        let state_param = pairs.next().unwrap();
        assert_eq!(state_param.0, Cow::Borrowed("state"));
        assert_eq!(state_param.1.len(), 22);
        assert_eq!(pairs.next(), Some((Cow::Borrowed("code_challenge_method"), Cow::Borrowed("S256"))));
        let code_challenge_param = pairs.next().unwrap();
        assert_eq!(code_challenge_param.0, Cow::Borrowed("code_challenge"));
        assert_eq!(code_challenge_param.1.len(), 43);
        assert_eq!(pairs.next(), Some((Cow::Borrowed("access_type"), Cow::Borrowed("offline"))));
        let keys_jwk = pairs.next().unwrap();
        assert_eq!(keys_jwk.0, Cow::Borrowed("keys_jwk"));
        assert_eq!(keys_jwk.1.len(), 168);
    }

    #[test]
    fn test_pairing_flow_origin_mismatch() {
        static PAIRING_URL: &'static str = "https://bad.origin.com/pair#channel_id=foo&channel_key=bar";
        let mut fxa = FirefoxAccount::new(Config::release("12345678", "https://foo.bar").unwrap());
        let url = fxa.begin_pairing_flow(&PAIRING_URL, &["https://identity.mozilla.com/apps/oldsync"]);

        assert!(url.is_err());

        match url {
            Ok(_) => { panic!("should have error"); }
            Err(err) => match err.kind() {
                ErrorKind::OriginMismatch { .. } => {},
                _ => panic!("error not OriginMismatch")
            }
        }
    }

    #[test]
    fn test_get_connection_success_url() {
        let fxa =
            FirefoxAccount::new(Config::stable_dev("12345678", "https://foo.bar").unwrap());
        let url = fxa.get_connection_success_url().unwrap().to_string();
        assert_eq!(url, "https://stable.dev.lcip.org/connect_another_device?showSuccessMessage=true".to_string());
    }
}

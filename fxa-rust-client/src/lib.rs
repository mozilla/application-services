/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate base64;
extern crate byteorder;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate hawk;
extern crate hex;
extern crate hkdf;
extern crate hmac;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate openssl;
extern crate rand;
extern crate regex;
extern crate reqwest;
extern crate ring;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate sha2;
extern crate untrusted;
extern crate url;

use std::collections::HashMap;
use std::mem;
use std::time::{SystemTime, UNIX_EPOCH};

use self::login_sm::LoginState::*;
use self::login_sm::*;
use byteorder::{ByteOrder, BigEndian};
use errors::*;
use http_client::browser_id::jwt_utils;
use http_client::{Client, OAuthTokenResponse, ProfileResponse};
use openssl::hash::{hash, MessageDigest};
use sha2::{Sha256, Digest};
use rand::{OsRng, RngCore};
use ring::{aead, agreement, rand as ring_rand};
use ring::agreement::EphemeralPrivateKey;
use untrusted::Input;
use url::Url;
use util::now;

mod config;
pub mod errors;
mod http_client;
mod login_sm;
mod oauth;
mod util;

pub use config::Config;
pub use http_client::ProfileResponse as Profile;

// If a cached token has less than `OAUTH_MIN_TIME_LEFT` seconds left to live,
// it will be considered already expired.
const OAUTH_MIN_TIME_LEFT: u64 = 60;
// A cached profile response is considered fresh for `PROFILE_FRESHNESS_THRESHOLD` ms.
const PROFILE_FRESHNESS_THRESHOLD: u64 = 120000; // 2 minutes

#[derive(Clone, Serialize, Deserialize)]
struct StateV1 {
    client_id: String,
    redirect_uri: String,
    config: Config,
    login_state: LoginState,
    oauth_cache: HashMap<String, OAuthInfo>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema_version")]
enum State {
    V1(StateV1),
}

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
    state: StateV1,
    flow_store: HashMap<String, OAuthFlow>,
    profile_cache: Option<CachedResponse<ProfileResponse>>,
}

pub type SyncKeys = (String, String);

impl FirefoxAccount {
    fn from_state(state: StateV1) -> FirefoxAccount {
        FirefoxAccount {
            state,
            flow_store: HashMap::new(),
            profile_cache: None,
        }
    }

    pub fn new(config: Config, client_id: &str, redirect_uri: &str) -> FirefoxAccount {
        FirefoxAccount::from_state(StateV1 {
            client_id: client_id.to_string(),
            redirect_uri: redirect_uri.to_string(),
            config,
            login_state: Unknown,
            oauth_cache: HashMap::new(),
        })
    }

    // Initialize state from Firefox Accounts credentials obtained using the
    // web flow.
    pub fn from_credentials(
        config: Config,
        client_id: &str,
        redirect_uri: &str,
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

        Ok(FirefoxAccount::from_state(StateV1 {
            client_id: client_id.to_string(),
            redirect_uri: redirect_uri.to_string(),
            config,
            login_state,
            oauth_cache: HashMap::new(),
        }))
    }

    pub fn from_json(data: &str) -> Result<FirefoxAccount> {
        let fxa_state: State = serde_json::from_str(data)?;
        match fxa_state {
            State::V1(state) => Ok(FirefoxAccount::from_state(state)),
        }
    }

    pub fn to_json(&self) -> Result<String> {
        let state = State::V1(self.state.clone());
        serde_json::to_string(&state).map_err(|e| e.into())
    }

    fn to_married(&mut self) -> Option<&MarriedState> {
        self.advance();
        match self.state.login_state {
            Married(ref married) => Some(married),
            _ => None,
        }
    }

    pub fn advance(&mut self) {
        let client = Client::new(&self.state.config);
        let state_machine = LoginStateMachine::new(client);
        let state = mem::replace(&mut self.state.login_state, Unknown);
        self.state.login_state = state_machine.advance(state);
    }

    fn oauth_cache_store(&mut self, info: &OAuthInfo) {
        let info = info.clone();
        let scope_key = info.scopes.join(" ");
        self.state.oauth_cache.insert(scope_key, info);
    }

    fn scope_implies_scopes(scope: &str, match_scopes: &[&str]) -> Result<bool> {
        let available_scopes = oauth::Scope::from_string(scope)?;
        for scope in match_scopes {
            let scope = oauth::Scope::from_string(scope)?;
            if !available_scopes.implies(&scope) {
                return Ok(false);
            }
        }
        return Ok(true);
    }

    fn oauth_cache_find(&self, requested_scopes: &[&str]) -> Option<&OAuthInfo> {
        // First we try to get the exact same scope.
        if let Some(info) = self.state.oauth_cache.get(&requested_scopes.join(" ")) {
            return Some(info);
        }
        for (scope_key, info) in self.state.oauth_cache.iter() {
            if FirefoxAccount::scope_implies_scopes(scope_key, requested_scopes).unwrap_or(false) {
                return Some(info);
            }
        }
        None
    }

    pub fn get_oauth_token(&mut self, scopes: &[&str]) -> Result<Option<OAuthInfo>> {
        let mut refresh_token = None;
        if let Some(cached_oauth_info) = self.oauth_cache_find(scopes) {
            if cached_oauth_info.expires_at > util::now_secs() + OAUTH_MIN_TIME_LEFT {
                return Ok(Some(cached_oauth_info.clone()));
            }
            refresh_token = cached_oauth_info.refresh_token.clone();
        }
        // This is a bit awkward, borrow checker weirdness.
        let resp;
        {
            if let Some(refresh_token) = refresh_token {
                let client = Client::new(&self.state.config);
                resp = client.oauth_token_with_refresh_token(
                    &self.state.client_id,
                    &refresh_token,
                    &scopes,
                )?;
            } else if let Some(session_token) =
                FirefoxAccount::session_token_from_state(&self.state.login_state)
            {
                let client = Client::new(&self.state.config);
                resp = client.oauth_token_with_session_token(
                    &self.state.client_id,
                    session_token,
                    &scopes,
                )?;
            } else {
                return Ok(None);
            }
        }
        Ok(Some(self.handle_oauth_token_response(resp, None)?))
    }

    pub fn begin_oauth_flow(&mut self, scopes: &[&str], wants_keys: bool) -> Result<String> {
        let state = FirefoxAccount::random_base64_url_string(16);
        let code_verifier = FirefoxAccount::random_base64_url_string(43);
        let code_challenge = hash(MessageDigest::sha256(), &code_verifier.as_bytes())?;
        let code_challenge = base64::encode_config(&code_challenge, base64::URL_SAFE_NO_PAD);
        let mut url = self.state.config.authorization_endpoint()?;
        url.query_pairs_mut()
            .append_pair("client_id", &self.state.client_id)
            .append_pair("redirect_uri", &self.state.redirect_uri)
            .append_pair("scope", &scopes.join(" "))
            .append_pair("response_type", "code")
            .append_pair("state", &state)
            .append_pair("code_challenge_method", "S256")
            .append_pair("code_challenge", &code_challenge)
            .append_pair("access_type", "offline");
        let jwk_prv_key = match wants_keys {
            true => {
                let rng = ring_rand::SystemRandom::new();
                let prv_key = EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng)?;
                let mut pub_key = vec![0u8; prv_key.public_key_len()];
                prv_key.compute_public_key(&mut pub_key)?;
                // First byte is 4, then 32 bytes for x, and 32 bytes for y.
                assert_eq!(pub_key.len(), 1 + 32 + 32);
                assert_eq!(pub_key[0], 0x04);
                let x = Vec::from(&pub_key[1..33]);
                let x = base64::encode_config(&x, base64::URL_SAFE_NO_PAD);
                let y = Vec::from(&pub_key[33..]);
                let y = base64::encode_config(&y, base64::URL_SAFE_NO_PAD);
                let jwk_json = json!({
                    "crv": "P-256",
                    "kty": "EC",
                    "x": x,
                    "y": y,
                }).to_string();
                let keys_jwk = base64::encode_config(&jwk_json, base64::URL_SAFE_NO_PAD);
                url.query_pairs_mut().append_pair("keys_jwk", &keys_jwk);
                Some(prv_key)
            }
            false => None,
        };
        self.flow_store.insert(
            state.clone(), // Since state is supposed to be unique, we use it to key our flows.
            OAuthFlow { jwk_prv_key, code_verifier },
        );
        Ok(url.to_string())
    }

    pub fn complete_oauth_flow(&mut self, code: &str, state: &str) -> Result<OAuthInfo> {
        let resp;
        // Needs non-lexical borrow checking.
        {
            let flow = match self.flow_store.get(state) {
                Some(flow) => flow,
                None => return Err(ErrorKind::UnknownOAuthState.into()),
            };
            let client = Client::new(&self.state.config);
            resp = client.oauth_token_with_code(&code, &flow.code_verifier, &self.state.client_id)?;
        }
        let oauth_flow = match self.flow_store.remove(state) {
            Some(oauth_flow) => oauth_flow,
            None => return Err(ErrorKind::UnknownOAuthState.into()),
        };
        self.handle_oauth_token_response(resp, oauth_flow.jwk_prv_key)
    }

    fn handle_oauth_token_response(
        &mut self,
        resp: OAuthTokenResponse,
        jwk_prv_key: Option<EphemeralPrivateKey>,
    ) -> Result<OAuthInfo> {
        let granted_scopes = resp.scope.split(" ").map(|s| s.to_string()).collect();
        // This assumes that if the server returns keys_jwe, the jwk argument is Some.
        let keys = match resp.keys_jwe {
            Some(jwe) => {
                let jwk_prv_key = jwk_prv_key.expect(
                    "Insane state! If we are getting back a JWE this means we should have a JWK private key.",
                );
                let segments: Vec<&str> = jwe.split(".").collect();
                let header = base64::decode_config(&segments[0], base64::URL_SAFE_NO_PAD)?;
                let protected_header: serde_json::Value = serde_json::from_slice(&header)?;
                assert_eq!(protected_header["epk"]["kty"], "EC");
                assert_eq!(protected_header["epk"]["crv"], "P-256");

                // Part 1: Grab the x/y from the other party and construct the secret.
                let x = base64::decode_config(&protected_header["epk"]["x"].as_str().unwrap(), base64::URL_SAFE_NO_PAD)?;
                let y = base64::decode_config(&protected_header["epk"]["y"].as_str().unwrap(), base64::URL_SAFE_NO_PAD)?;
                let mut peer_pub_key: Vec<u8> = vec![0x04];
                peer_pub_key.extend_from_slice(&x);
                peer_pub_key.extend_from_slice(&y);
                let peer_pub_key = Input::from(&peer_pub_key);
                let secret = agreement::agree_ephemeral(jwk_prv_key, &agreement::ECDH_P256, peer_pub_key, ring::error::Unspecified, |z| {
                    // ConcatKDF
                    let counter = 1;
                    let alg = protected_header["enc"].as_str().unwrap();
                    let apu = "";
                    let apv = "";
                    let mut buf: Vec<u8> = vec![];
                    buf.extend_from_slice(&to_32b_buf(counter));
                    buf.extend_from_slice(&z);
                    buf.extend_from_slice(&to_32b_buf(alg.len() as u32));
                    buf.extend_from_slice(alg.as_bytes());
                    buf.extend_from_slice(&to_32b_buf(apu.len() as u32));
                    buf.extend_from_slice(apu.as_bytes());
                    buf.extend_from_slice(&to_32b_buf(apv.len() as u32));
                    buf.extend_from_slice(apv.as_bytes());
                    buf.extend_from_slice(&to_32b_buf(256));
                    Ok(Sha256::digest(&buf)[0..32].to_vec())
                })?;

                // Part 2: decrypt the payload with the obtained secret
                assert_eq!(segments[1].len(), 0); // Encrypted Key is zero-length.
                let iv = base64::decode_config(&segments[2], base64::URL_SAFE_NO_PAD)?;
                let ciphertext = base64::decode_config(&segments[3], base64::URL_SAFE_NO_PAD)?;
                let auth_tag = base64::decode_config(&segments[4], base64::URL_SAFE_NO_PAD)?;
                assert_eq!(auth_tag.len(), 128 / 8);
                assert_eq!(iv.len(), 96 / 8);
                let opening_key = aead::OpeningKey::new(&aead::AES_256_GCM, &secret)?;
                let mut in_out = ciphertext.to_vec();
                in_out.append(&mut auth_tag.to_vec());
                let plaintext = aead::open_in_place(
                    &opening_key,
                    &iv,
                    segments[0].as_bytes(),
                    0,
                    &mut in_out,
                )?;
                let plaintext = String::from_utf8(plaintext.to_vec())?;
                Some(plaintext)
            }
            None => {
                if jwk_prv_key.is_some() {
                    error!("Expected to get keys back alongside the token but the server didn't send them.");
                    return Err(ErrorKind::TokenWithoutKeys.into());
                } else {
                    None
                }
            }
        };
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Something is very wrong.");
        let expires_at = since_epoch.as_secs() + resp.expires_in;
        let oauth_info = OAuthInfo {
            access_token: resp.access_token,
            keys,
            refresh_token: resp.refresh_token,
            expires_at,
            scopes: granted_scopes,
        };
        self.oauth_cache_store(&oauth_info);
        Ok(oauth_info)
    }

    fn random_base64_url_string(len: usize) -> String {
        let mut r = OsRng::new().expect("Could not instantiate RNG");
        let mut buf: Vec<u8> = vec![0; len];
        r.fill_bytes(buf.as_mut_slice());
        let random = base64::encode_config(&buf, base64::URL_SAFE_NO_PAD);
        random
    }

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
        let profile_access_token = match self.get_oauth_token(&["profile"])? {
            Some(token) => token.access_token,
            None => return Err(ErrorKind::NoCachedToken("profile").into()),
        };
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

    pub fn get_sync_keys(&mut self) -> Result<SyncKeys> {
        let married = match self.to_married() {
            Some(married) => married,
            None => return Err(ErrorKind::NotMarried.into()),
        };
        let sync_key = hex::encode(married.sync_key());
        Ok((sync_key, married.xcs().to_string()))
    }

    pub fn get_token_server_endpoint_url(&self) -> Result<Url> {
        self.state.config.token_server_endpoint_url()
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

    pub fn sign_out(mut self) {
        let client = Client::new(&self.state.config);
        client.sign_out();
        self.state.login_state = self.state.login_state.to_separated();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize() {
        let config = Config::stable_dev().unwrap();
        let fxa1 = FirefoxAccount::from_credentials(
            config,
            "5882386c6d801776",
            "https://foo.bar",
            WebChannelResponse {
                uid: "123456".to_string(),
                email: "foo@bar.com".to_string(),
                verified: false,
                session_token: "12".to_string(),
                key_fetch_token: "34".to_string(),
                unwrap_kb: "56".to_string(),
            },
        ).unwrap();
        let fxa1_json = fxa1.to_json().unwrap();
        drop(fxa1);
        let fxa2 = FirefoxAccount::from_json(&fxa1_json).unwrap();
        let fxa2_json = fxa2.to_json().unwrap();
        assert_eq!(fxa1_json, fxa2_json);
    }

    #[test]
    fn test_oauth_cache_store_and_find() {
        let mut fxa =
            FirefoxAccount::new(Config::stable_dev().unwrap(), "12345678", "https://foo.bar");
        let oauth_info = OAuthInfo {
            access_token: "abcdef".to_string(),
            keys: None,
            refresh_token: None,
            expires_at: 1,
            scopes: vec![
                "profile".to_string(),
                "https://identity.mozilla.com/apps/oldsync".to_string(),
            ],
        };
        fxa.oauth_cache_store(&oauth_info);
        fxa.oauth_cache_find(&["profile"]).unwrap();
    }
}

fn to_32b_buf(n: u32) -> Vec<u8> {
    let mut buf = [0; 4];
    BigEndian::write_u32(&mut buf, n);
    buf.to_vec()
}

pub struct OAuthFlow {
    pub jwk_prv_key: Option<EphemeralPrivateKey>,
    pub code_verifier: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct OAuthInfo {
    pub access_token: String,
    pub keys: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: u64, // seconds since epoch
    pub scopes: Vec<String>,
}

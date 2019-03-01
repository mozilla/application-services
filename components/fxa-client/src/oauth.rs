/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    errors::*, http_client::OAuthTokenResponse, scoped_keys::ScopedKeysFlow, util, FirefoxAccount,
    RNG,
};
use rc_crypto::digest;
use serde_derive::*;
use std::{
    collections::HashSet,
    iter::FromIterator,
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;

// If a cached token has less than `OAUTH_MIN_TIME_LEFT` seconds left to live,
// it will be considered already expired.
const OAUTH_MIN_TIME_LEFT: u64 = 60;

impl FirefoxAccount {
    /// Fetch a short-lived access token using the saved refresh token.
    /// If there is no refresh token held or if it is not authorized for some of the requested
    /// scopes, this method will error-out and a login flow will need to be initiated
    /// using `begin_oauth_flow`.
    ///
    /// * `scopes` - Space-separated list of requested scopes.
    pub fn get_access_token(&mut self, scope: &str) -> Result<AccessTokenInfo> {
        if scope.contains(' ') {
            return Err(ErrorKind::MultipleScopesRequested.into());
        }
        if let Some(oauth_info) = self.access_token_cache.get(scope) {
            if oauth_info.expires_at > util::now_secs() + OAUTH_MIN_TIME_LEFT {
                return Ok(oauth_info.clone());
            }
        }
        let resp = match self.state.refresh_token {
//            Some(ref refresh_token) => {
//                if refresh_token.scopes.contains(scope) {
//                    self.client.oauth_token_with_refresh_token(
//                        &self.state.config,
//                        &refresh_token.token,
//                        &[scope],
//                    )?
//                } else {
//                    return Err(ErrorKind::NoCachedToken(scope.to_string()).into());
//                }
//            }
            // TODO: FOR THE SAKE OF THE PR AND AS A ONE-TIME DEAL FOR VLAD,
            // I REMOVED THE SCOPE CHECK.
            Some(ref refresh_token) => //match refresh_token.scopes.contains(scope) {
                /*true =>*/ self.client.oauth_token_with_refresh_token(
                    &self.state.config,
                    &refresh_token.token,
                    &[scope],
                )?,
                //false => return Err(ErrorKind::NoCachedToken(scope.to_string()).into()),
            //},
            None => {
                #[cfg(feature = "browserid")]
                {
                    match Self::session_token_from_state(&self.state.login_state) {
                        Some(session_token) => self.client.oauth_token_with_session_token(
                            &self.state.config,
                            session_token,
                            &[scope],
                        )?,
                        None => return Err(ErrorKind::NoCachedToken(scope.to_string()).into()),
                    }
                }
                #[cfg(not(feature = "browserid"))]
                {
                    return Err(ErrorKind::NoCachedToken(scope.to_string()).into());
                }
            }
        };
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ErrorKind::IllegalState("Current date before Unix Epoch.".to_string()))?;
        let expires_at = since_epoch.as_secs() + resp.expires_in;
        let token_info = AccessTokenInfo {
            scope: resp.scope,
            token: resp.access_token,
            key: self.state.scoped_keys.get(scope).cloned(),
            expires_at,
        };
        self.access_token_cache
            .insert(scope.to_string(), token_info.clone());
        Ok(token_info)
    }

    /// Initiate a pairing flow and return a URL that should be navigated to.
    ///
    /// * `pairing_url` - A pairing URL obtained by scanning a QR code produced by
    /// the pairing authority.
    /// * `scopes` - Space-separated list of requested scopes by the pairing supplicant.
    pub fn begin_pairing_flow(&mut self, pairing_url: &str, scopes: &[&str]) -> Result<String> {
        let mut url = self.state.config.content_url_path("/pair/supp")?;
        let pairing_url = Url::parse(pairing_url)?;
        if url.host_str() != pairing_url.host_str() {
            return Err(ErrorKind::OriginMismatch.into());
        }
        url.set_fragment(pairing_url.fragment());
        self.oauth_flow(url, scopes, true)
    }

    /// Initiate an OAuth login flow and return a URL that should be navigated to.
    ///
    /// * `scopes` - Space-separated list of requested scopes.
    /// * `wants_keys` - Retrieve scoped keys associated with scopes supporting it.
    pub fn begin_oauth_flow(&mut self, scopes: &[&str], wants_keys: bool) -> Result<String> {
        let mut url = self.state.config.authorization_endpoint()?;
        url.query_pairs_mut()
            .append_pair("action", "email")
            .append_pair("response_type", "code");
        let scopes: Vec<String> = match self.state.refresh_token {
            Some(ref refresh_token) => {
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
        self.oauth_flow(url, &scopes, wants_keys)
    }

    fn oauth_flow(&mut self, mut url: Url, scopes: &[&str], wants_keys: bool) -> Result<String> {
        let state = util::random_base64_url_string(&*RNG, 16)?;
        let code_verifier = util::random_base64_url_string(&*RNG, 43)?;
        let code_challenge = digest::digest(&digest::SHA256, &code_verifier.as_bytes())?;
        let code_challenge = base64::encode_config(&code_challenge, base64::URL_SAFE_NO_PAD);
        url.query_pairs_mut()
            .append_pair("client_id", &self.state.config.client_id)
            .append_pair("redirect_uri", &self.state.config.redirect_uri)
            .append_pair("scope", &scopes.join(" "))
            .append_pair("state", &state)
            .append_pair("code_challenge_method", "S256")
            .append_pair("code_challenge", &code_challenge)
            .append_pair("access_type", "offline");
        let scoped_keys_flow = if wants_keys {
            let flow = ScopedKeysFlow::with_random_key(&*RNG)?;
            let jwk_json = flow.generate_keys_jwk()?;
            let keys_jwk = base64::encode_config(&jwk_json, base64::URL_SAFE_NO_PAD);
            url.query_pairs_mut().append_pair("keys_jwk", &keys_jwk);
            Some(flow)
        } else {
            None
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

    /// Complete an OAuth flow initiated in `begin_oauth_flow` or `begin_pairing_flow`.
    /// The `code` and `state` parameters can be obtained by parsing out the
    /// redirect URL after a successful login.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn complete_oauth_flow(&mut self, code: &str, state: &str) -> Result<()> {
        let oauth_flow = match self.flow_store.remove(state) {
            Some(oauth_flow) => oauth_flow,
            None => return Err(ErrorKind::UnknownOAuthState.into()),
        };
        let resp = self.client.oauth_token_with_code(
            &self.state.config,
            &code,
            &oauth_flow.code_verifier,
        )?;
        self.handle_oauth_response(resp, oauth_flow.scoped_keys_flow)
    }

    pub(crate) fn handle_oauth_response(
        &mut self,
        resp: OAuthTokenResponse,
        scoped_keys_flow: Option<ScopedKeysFlow>,
    ) -> Result<()> {
        // This assumes that if the server returns keys_jwe, the jwk argument is Some.
        match resp.keys_jwe {
            Some(ref jwe) => {
                let scoped_keys_flow = match scoped_keys_flow {
                    Some(flow) => flow,
                    None => {
                        return Err(ErrorKind::UnrecoverableServerError(
                            "Got a JWE without sending a JWK.",
                        )
                        .into());
                    }
                };
                let decrypted_keys = scoped_keys_flow.decrypt_keys_jwe(jwe)?;
                let scoped_keys: serde_json::Map<String, serde_json::Value> =
                    serde_json::from_str(&decrypted_keys)?;
                for (scope, key) in scoped_keys {
                    let scoped_key: ScopedKey = serde_json::from_value(key)?;
                    self.state.scoped_keys.insert(scope, scoped_key);
                }
            }
            None => {
                if scoped_keys_flow.is_some() {
                    log::error!("Expected to get keys back alongside the token but the server didn't send them.");
                    return Err(ErrorKind::TokenWithoutKeys.into());
                }
            }
        };
        // We are only interested in the refresh token at this time because we
        // don't want to return an over-scoped access token.
        // Let's be good citizens and destroy this access token.
        if let Err(err) = self
            .client
            .destroy_oauth_token(&self.state.config, &resp.access_token)
        {
            log::warn!("Access token destruction failure: {:?}", err);
        }
        let refresh_token = match resp.refresh_token {
            Some(ref refresh_token) => refresh_token.clone(),
            None => return Err(ErrorKind::RefreshTokenNotPresent.into()),
        };
        // In order to keep 1 and only 1 refresh token alive per client instance,
        // we also destroy the existing refresh token.
        if let Some(ref old_refresh_token) = self.state.refresh_token {
            if let Err(err) = self
                .client
                .destroy_oauth_token(&self.state.config, &old_refresh_token.token)
            {
                log::warn!("Refresh token destruction failure: {:?}", err);
            }
        }
        self.state.refresh_token = Some(RefreshToken {
            token: refresh_token,
            scopes: HashSet::from_iter(resp.scope.split(' ').map(ToString::to_string)),
        });
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScopedKey {
    pub kty: String,
    pub scope: String,
    /// URL Safe Base 64 encoded key.
    pub k: String,
    pub kid: String,
}

impl ScopedKey {
    pub fn key_bytes(&self) -> Result<Vec<u8>> {
        Ok(base64::decode_config(&self.k, base64::URL_SAFE_NO_PAD)?)
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
    pub key: Option<ScopedKey>,
    pub expires_at: u64, // seconds since epoch
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn test_oauth_flow_url() {
        let mut fxa = FirefoxAccount::new(
            "https://accounts.firefox.com",
            "12345678",
            "https://foo.bar",
        );
        let url = fxa.begin_oauth_flow(&["profile"], false).unwrap();
        let flow_url = Url::parse(&url).unwrap();

        assert_eq!(flow_url.host_str(), Some("accounts.firefox.com"));
        assert_eq!(flow_url.path(), "/authorization");

        let mut pairs = flow_url.query_pairs();
        assert_eq!(pairs.count(), 9);
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
            Some((Cow::Borrowed("client_id"), Cow::Borrowed("12345678")))
        );
        assert_eq!(
            pairs.next(),
            Some((
                Cow::Borrowed("redirect_uri"),
                Cow::Borrowed("https://foo.bar")
            ))
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
    }

    #[test]
    fn test_oauth_flow_url_with_keys() {
        let mut fxa = FirefoxAccount::new(
            "https://accounts.firefox.com",
            "12345678",
            "https://foo.bar",
        );
        let url = fxa.begin_oauth_flow(&["profile"], true).unwrap();
        let flow_url = Url::parse(&url).unwrap();

        assert_eq!(flow_url.host_str(), Some("accounts.firefox.com"));
        assert_eq!(flow_url.path(), "/authorization");

        let mut pairs = flow_url.query_pairs();
        assert_eq!(pairs.count(), 10);
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
            Some((Cow::Borrowed("client_id"), Cow::Borrowed("12345678")))
        );
        assert_eq!(
            pairs.next(),
            Some((
                Cow::Borrowed("redirect_uri"),
                Cow::Borrowed("https://foo.bar")
            ))
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
    }

    #[test]
    fn test_pairing_flow_url() {
        const SCOPES: &[&str] = &["https://identity.mozilla.com/apps/oldsync"];
        const PAIRING_URL: &str = "https://accounts.firefox.com/pair#channel_id=658db7fe98b249a5897b884f98fb31b7&channel_key=1hIDzTj5oY2HDeSg_jA2DhcOcAn5Uqq0cAYlZRNUIo4";
        const EXPECTED_URL: &str = "https://accounts.firefox.com/pair/supp?client_id=12345678&redirect_uri=https%3A%2F%2Ffoo.bar&scope=https%3A%2F%2Fidentity.mozilla.com%2Fapps%2Foldsync&state=SmbAA_9EA5v1R2bgIPeWWw&code_challenge_method=S256&code_challenge=ZgHLPPJ8XYbXpo7VIb7wFw0yXlTa6MUOVfGiADt0JSM&access_type=offline&keys_jwk=eyJjcnYiOiJQLTI1NiIsImt0eSI6IkVDIiwieCI6Ing5LUltQjJveDM0LTV6c1VmbW5sNEp0Ti14elV2eFZlZXJHTFRXRV9BT0kiLCJ5IjoiNXBKbTB3WGQ4YXdHcm0zREl4T1pWMl9qdl9tZEx1TWlMb1RkZ1RucWJDZyJ9#channel_id=658db7fe98b249a5897b884f98fb31b7&channel_key=1hIDzTj5oY2HDeSg_jA2DhcOcAn5Uqq0cAYlZRNUIo4";

        let mut fxa = FirefoxAccount::new(
            "https://accounts.firefox.com",
            "12345678",
            "https://foo.bar",
        );
        let url = fxa.begin_pairing_flow(&PAIRING_URL, &SCOPES).unwrap();
        let flow_url = Url::parse(&url).unwrap();
        let expected_parsed_url = Url::parse(EXPECTED_URL).unwrap();

        assert_eq!(flow_url.host_str(), Some("accounts.firefox.com"));
        assert_eq!(flow_url.path(), "/pair/supp");
        assert_eq!(flow_url.fragment(), expected_parsed_url.fragment());

        let mut pairs = flow_url.query_pairs();
        assert_eq!(pairs.count(), 8);
        assert_eq!(
            pairs.next(),
            Some((Cow::Borrowed("client_id"), Cow::Borrowed("12345678")))
        );
        assert_eq!(
            pairs.next(),
            Some((
                Cow::Borrowed("redirect_uri"),
                Cow::Borrowed("https://foo.bar")
            ))
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
    }

    #[test]
    fn test_pairing_flow_origin_mismatch() {
        static PAIRING_URL: &'static str =
            "https://bad.origin.com/pair#channel_id=foo&channel_key=bar";
        let mut fxa = FirefoxAccount::new(
            "https://accounts.firefox.com",
            "12345678",
            "https://foo.bar",
        );
        let url =
            fxa.begin_pairing_flow(&PAIRING_URL, &["https://identity.mozilla.com/apps/oldsync"]);

        assert!(url.is_err());

        match url {
            Ok(_) => {
                panic!("should have error");
            }
            Err(err) => match err.kind() {
                ErrorKind::OriginMismatch { .. } => {}
                _ => panic!("error not OriginMismatch"),
            },
        }
    }
}

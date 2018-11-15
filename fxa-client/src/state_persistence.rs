/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::collections::HashSet;
use std::iter::FromIterator;
use serde_json;
use errors::*;
use config::Config;
use StateV2;
use RefreshToken;
type State = StateV2;

pub(crate) fn state_from_json(data: &str) -> Result<State> {
    let stored_state: PersistedState = serde_json::from_str(data)?;
    return upgrade_state(stored_state)
}

pub(crate) fn state_to_json(state: &State) -> Result<String> {
    let state = PersistedState::V2(state.clone());
    serde_json::to_string(&state).map_err(|e| e.into())
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema_version")]
enum PersistedState {
    #[serde(skip_serializing)]
    V1(StateV1),
    V2(StateV2),
}

fn upgrade_state(in_state: PersistedState) -> Result<State> {
    return match in_state {
        PersistedState::V1(state) => state.into(),
        PersistedState::V2(state) => Ok(state),
    }
}

// Migrations
impl From<StateV1> for Result<StateV2> {
    fn from(state: StateV1) -> Self {
        let mut all_refresh_tokens: Vec<V1AuthInfo> = vec![];
        let mut all_scoped_keys = HashMap::new();
        for access_token in state.oauth_cache.values() {
            if access_token.refresh_token.is_some() {
                all_refresh_tokens.push(access_token.clone());
            }
            if let Some(ref scoped_keys) = access_token.keys {
                let scoped_keys: serde_json::Map<String, serde_json::Value> = serde_json::from_str(scoped_keys)?;
                for (scope, key) in scoped_keys {
                    all_scoped_keys.insert(scope.clone(), key.clone());
                }
            }
        }
        // In StateV2 we hold one and only one refresh token.
        // Obviously this means a loss of information.
        // Heuristic: We keep the most recent token.
        let refresh_token = all_refresh_tokens.iter()
            .max_by(|a, b| a.expires_at.cmp(&b.expires_at))
            .map(|token| RefreshToken {
                token: token.refresh_token.clone().expect("all_refresh_tokens should only contain access tokens with refresh tokens"),
                scopes: HashSet::from_iter(token.scopes.iter().map(|s| s.to_string())),
            });
        Ok(StateV2 {
            config: Config::new(
                state.config.content_url,
                state.config.auth_url,
                state.config.oauth_url,
                state.config.profile_url,
                state.config.token_server_endpoint_url,
                state.config.authorization_endpoint,
                state.config.issuer,
                state.config.jwks_uri,
                state.config.token_endpoint,
                state.config.userinfo_endpoint,
                state.client_id,
                state.redirect_uri,
            ),
            #[cfg(feature = "browserid")]
            login_state: super::login_sm::LoginState::Unknown,
            refresh_token,
            scoped_keys: all_scoped_keys,
        })
    }
}

// Older data structures used for migration purposes.
#[derive(Deserialize)]
struct StateV1 {
    client_id: String,
    redirect_uri: String,
    config: V1Config,
    // #[cfg(feature = "browserid")]
    // login_state: LoginState, // Wasn't used anyway
    oauth_cache: HashMap<String, V1AuthInfo>,
}

#[derive(Deserialize)]
struct V1Config {
    content_url: String,
    auth_url: String,
    oauth_url: String,
    profile_url: String,
    token_server_endpoint_url: String,
    authorization_endpoint: String,
    issuer: String,
    jwks_uri: String,
    token_endpoint: String,
    userinfo_endpoint: String,
}

#[derive(Deserialize, Clone)]
struct V1AuthInfo {
    pub access_token: String,
    pub keys: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: u64, // seconds since epoch
    pub scopes: Vec<String>,
}

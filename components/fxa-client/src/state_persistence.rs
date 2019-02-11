/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{config::Config, errors::*, RefreshToken, ScopedKey, StateV2};
use serde_derive::*;
use std::{
    collections::{HashMap, HashSet},
    iter::FromIterator,
};
type State = StateV2;

pub(crate) fn state_from_json(data: &str) -> Result<State> {
    let stored_state: PersistedState = serde_json::from_str(data)?;
    upgrade_state(stored_state)
}

pub(crate) fn state_to_json(state: &State) -> Result<String> {
    let state = PersistedState::V2(state.clone());
    serde_json::to_string(&state).map_err(Into::into)
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema_version")]
enum PersistedState {
    #[serde(skip_serializing)]
    V1(StateV1),
    V2(StateV2),
}

fn upgrade_state(in_state: PersistedState) -> Result<State> {
    match in_state {
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
                let scoped_keys: serde_json::Map<String, serde_json::Value> =
                    serde_json::from_str(scoped_keys)?;
                for (scope, key) in scoped_keys {
                    let scoped_key: ScopedKey = serde_json::from_value(key)?;
                    all_scoped_keys.insert(scope, scoped_key);
                }
            }
        }
        // In StateV2 we hold one and only one refresh token.
        // Obviously this means a loss of information.
        // Heuristic: We keep the most recent token.
        let refresh_token = all_refresh_tokens
            .iter()
            .max_by(|a, b| a.expires_at.cmp(&b.expires_at))
            .map(|token| RefreshToken {
                token: token.refresh_token.clone().expect(
                    "all_refresh_tokens should only contain access tokens with refresh tokens",
                ),
                scopes: HashSet::from_iter(token.scopes.iter().map(ToString::to_string)),
            });
        Ok(StateV2 {
            config: Config::init(
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
            last_handled_command: None,
            commands_data: HashMap::new(),
            device_capabilities: HashSet::new(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v1_migration() {
        let state_v1_json = "{\"schema_version\":\"V1\",\"client_id\":\"98adfa37698f255b\",\"redirect_uri\":\"https://lockbox.firefox.com/fxa/ios-redirect.html\",\"config\":{\"content_url\":\"https://accounts.firefox.com\",\"auth_url\":\"https://api.accounts.firefox.com/\",\"oauth_url\":\"https://oauth.accounts.firefox.com/\",\"profile_url\":\"https://profile.accounts.firefox.com/\",\"token_server_endpoint_url\":\"https://token.services.mozilla.com/1.0/sync/1.5\",\"authorization_endpoint\":\"https://accounts.firefox.com/authorization\",\"issuer\":\"https://accounts.firefox.com\",\"jwks_uri\":\"https://oauth.accounts.firefox.com/v1/jwks\",\"token_endpoint\":\"https://oauth.accounts.firefox.com/v1/token\",\"userinfo_endpoint\":\"https://profile.accounts.firefox.com/v1/profile\"},\"oauth_cache\":{\"https://identity.mozilla.com/apps/oldsync https://identity.mozilla.com/apps/lockbox profile\":{\"access_token\":\"bef37ec0340783356bcac67a86c4efa23a56f2ddd0c7a6251d19988bab7bdc99\",\"keys\":\"{\\\"https://identity.mozilla.com/apps/oldsync\\\":{\\\"kty\\\":\\\"oct\\\",\\\"scope\\\":\\\"https://identity.mozilla.com/apps/oldsync\\\",\\\"k\\\":\\\"kMtwpVC0ZaYFJymPza8rXK_0CgCp3KMwRStwGfBRBDtL6hXRDVJgQFaoOQ2dimw0Bko5WVv2gNTy7RX5zFYZHg\\\",\\\"kid\\\":\\\"1542236016429-Ox1FbJfFfwTe5t-xq4v2hQ\\\"},\\\"https://identity.mozilla.com/apps/lockbox\\\":{\\\"kty\\\":\\\"oct\\\",\\\"scope\\\":\\\"https://identity.mozilla.com/apps/lockbox\\\",\\\"k\\\":\\\"Qk4K4xF2PgQ6XvBXW8X7B7AWwWgW2bHQov9NHNd4v-k\\\",\\\"kid\\\":\\\"1231014287-KDVj0DFaO3wGpPJD8oPwVg\\\"}}\",\"refresh_token\":\"bed5532f4fea7e39c5c4f609f53603ee7518fd1c103cc4034da3618f786ed188\",\"expires_at\":1543474657,\"scopes\":[\"https://identity.mozilla.com/apps/oldsync\",\"https://identity.mozilla.com/apps/lockbox\",\"profile\"]}}}";
        let state = state_from_json(state_v1_json).unwrap();
        assert!(state.refresh_token.is_some());
        let refresh_token = state.refresh_token.unwrap();
        assert_eq!(
            refresh_token.token,
            "bed5532f4fea7e39c5c4f609f53603ee7518fd1c103cc4034da3618f786ed188"
        );
        assert_eq!(refresh_token.scopes.len(), 3);
        assert!(refresh_token.scopes.contains("profile"));
        assert!(refresh_token
            .scopes
            .contains("https://identity.mozilla.com/apps/oldsync"));
        assert!(refresh_token
            .scopes
            .contains("https://identity.mozilla.com/apps/lockbox"));
        assert_eq!(state.scoped_keys.len(), 2);
        let oldsync_key = &state.scoped_keys["https://identity.mozilla.com/apps/oldsync"];
        assert_eq!(oldsync_key.kid, "1542236016429-Ox1FbJfFfwTe5t-xq4v2hQ");
        assert_eq!(oldsync_key.k, "kMtwpVC0ZaYFJymPza8rXK_0CgCp3KMwRStwGfBRBDtL6hXRDVJgQFaoOQ2dimw0Bko5WVv2gNTy7RX5zFYZHg");
        assert_eq!(oldsync_key.kty, "oct");
        assert_eq!(
            oldsync_key.scope,
            "https://identity.mozilla.com/apps/oldsync"
        );
        let lockbox_key = &state.scoped_keys["https://identity.mozilla.com/apps/lockbox"];

        assert_eq!(lockbox_key.kid, "1231014287-KDVj0DFaO3wGpPJD8oPwVg");
        assert_eq!(lockbox_key.k, "Qk4K4xF2PgQ6XvBXW8X7B7AWwWgW2bHQov9NHNd4v-k");
        assert_eq!(lockbox_key.kty, "oct");
        assert_eq!(
            lockbox_key.scope,
            "https://identity.mozilla.com/apps/lockbox"
        );
    }
}

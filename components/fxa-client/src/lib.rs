/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[cfg(feature = "browserid")]
pub use crate::browser_id::{SyncKeys, WebChannelResponse};
#[cfg(feature = "browserid")]
use crate::login_sm::LoginState;
pub use crate::{config::Config, http_client::ProfileResponse as Profile, oauth::AccessTokenInfo};
use crate::{
    errors::*,
    http_client::Client,
    oauth::{OAuthFlow, RefreshToken, ScopedKey},
};
use lazy_static::lazy_static;
use ring::rand::SystemRandom;
use serde_derive::*;
use std::{collections::HashMap, panic::RefUnwindSafe};
use url::Url;

#[cfg(feature = "browserid")]
mod browser_id;
mod config;
pub mod errors;
#[cfg(feature = "ffi")]
pub mod ffi;
mod http_client;
#[cfg(feature = "browserid")]
mod login_sm;
mod oauth;
mod scoped_keys;
pub mod scopes;
mod state_persistence;
mod util;

// A cached profile response is considered fresh for `PROFILE_FRESHNESS_THRESHOLD` ms.
const PROFILE_FRESHNESS_THRESHOLD: u64 = 120000; // 2 minutes

lazy_static! {
    static ref RNG: SystemRandom = SystemRandom::new();
}

// If this structure is modified, please
// check whether or not a migration needs to be done
// as these fields are persisted as a JSON string
// (see `state_persistence.rs`).
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StateV2 {
    config: Config,
    #[cfg(feature = "browserid")]
    login_state: LoginState,
    refresh_token: Option<RefreshToken>,
    scoped_keys: HashMap<String, ScopedKey>,
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
    profile_cache: Option<CachedResponse<Profile>>,
}

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
    fn from_state(state: StateV2) -> Self {
        Self {
            state,
            access_token_cache: HashMap::new(),
            flow_store: HashMap::new(),
            persist_callback: None,
            profile_cache: None,
        }
    }

    pub fn with_config(config: Config) -> Self {
        Self::from_state(StateV2 {
            config,
            #[cfg(feature = "browserid")]
            login_state: LoginState::Unknown,
            refresh_token: None,
            scoped_keys: HashMap::new(),
        })
    }

    pub fn new(content_url: &str, client_id: &str, redirect_uri: &str) -> Self {
        let config = Config::new(content_url, client_id, redirect_uri);
        Self::with_config(config)
    }

    pub fn from_json(data: &str) -> Result<Self> {
        let state = state_persistence::state_from_json(data)?;
        Ok(Self::from_state(state))
    }

    pub fn to_json(&self) -> Result<String> {
        state_persistence::state_to_json(&self.state)
    }

    pub fn get_profile(&mut self, ignore_cache: bool) -> Result<Profile> {
        let profile_access_token = self.get_access_token(scopes::PROFILE)?.token;
        let mut etag = None;
        if let Some(ref cached_profile) = self.profile_cache {
            if !ignore_cache && util::now() < cached_profile.cached_at + PROFILE_FRESHNESS_THRESHOLD
            {
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
                        cached_at: util::now(),
                        etag,
                    });
                }
                Ok(response_and_etag.response)
            }
            None => match self.profile_cache {
                Some(ref cached_profile) => Ok(cached_profile.response.clone()),
                None => Err(ErrorKind::UnrecoverableServerError(
                    "Got a 304 without having sent an eTag.",
                )
                .into()),
            },
        }
    }

    pub fn get_token_server_endpoint_url(&self) -> Result<Url> {
        self.state.config.token_server_endpoint_url()
    }

    pub fn get_connection_success_url(&self) -> Result<Url> {
        let mut url = self
            .state
            .config
            .content_url_path("connect_another_device")?;
        url.query_pairs_mut()
            .append_pair("showSuccessMessage", "true");
        Ok(url)
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
                    log::error!("Error with to_json in persist_callback");
                    return;
                }
            };
            cb.call(&json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fxa_is_send() {
        fn is_send<T: Send>() {}
        is_send::<FirefoxAccount>();
    }

    #[test]
    fn test_serialize_deserialize() {
        let fxa1 =
            FirefoxAccount::new("https://stable.dev.lcip.org", "12345678", "https://foo.bar");
        let fxa1_json = fxa1.to_json().unwrap();
        drop(fxa1);
        let fxa2 = FirefoxAccount::from_json(&fxa1_json).unwrap();
        let fxa2_json = fxa2.to_json().unwrap();
        assert_eq!(fxa1_json, fxa2_json);
    }

    #[test]
    fn test_get_connection_success_url() {
        let fxa = FirefoxAccount::new("https://stable.dev.lcip.org", "12345678", "https://foo.bar");
        let url = fxa.get_connection_success_url().unwrap().to_string();
        assert_eq!(
            url,
            "https://stable.dev.lcip.org/connect_another_device?showSuccessMessage=true"
                .to_string()
        );
    }
}

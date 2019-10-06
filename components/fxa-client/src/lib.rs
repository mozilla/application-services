/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

use crate::{
    commands::send_tab::SendTabPayload,
    device::{Capability as DeviceCapability, Device},
    oauth::{OAuthFlow, RefreshToken},
    scoped_keys::ScopedKey,
};
pub use crate::{
    config::Config, error::*, oauth::AccessTokenInfo, oauth::IntrospectInfo, profile::Profile,
};
use serde_derive::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use url::Url;

mod commands;
mod config;
pub mod device;
pub mod error;
pub mod ffi;
mod migrator;
// Include the `msg_types` module, which is generated from msg_types.proto.
pub mod msg_types {
    include!(concat!(env!("OUT_DIR"), "/msg_types.rs"));
}
mod http_client;
mod oauth;
mod profile;
mod scoped_keys;
pub mod scopes;
pub mod send_tab;
mod state_persistence;
mod util;

type FxAClient = dyn http_client::FxAClient + Sync + Send;

// FIXME: https://github.com/myelin-ai/mockiato/issues/106.
#[cfg(test)]
unsafe impl<'a> Send for http_client::FxAClientMock<'a> {}
#[cfg(test)]
unsafe impl<'a> Sync for http_client::FxAClientMock<'a> {}

// It this struct is modified, please check if the
// `FirefoxAccount.start_over` function also needs
// to be modified.
pub struct FirefoxAccount {
    client: Arc<FxAClient>,
    state: StateV2,
    flow_store: HashMap<String, OAuthFlow>,
}

// If this structure is modified, please:
// 1. Check if a migration needs to be done, as
// these fields are persisted as a JSON string
// (see `state_persistence.rs`).
// 2. Check if the `StateVX.start_over` function
// also needs to to be modified.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StateV2 {
    config: Config,
    current_device_id: Option<String>,
    refresh_token: Option<RefreshToken>,
    scoped_keys: HashMap<String, ScopedKey>,
    last_handled_command: Option<u64>,
    // Remove serde(default) once we are V3.
    #[serde(default)]
    commands_data: HashMap<String, String>,
    #[serde(default)] // Same
    device_capabilities: HashSet<DeviceCapability>,
    #[serde(default)] // Same
    access_token_cache: HashMap<String, AccessTokenInfo>,
    session_token: Option<String>, // Hex-formatted string.
    last_seen_profile: Option<CachedResponse<Profile>>,
}

impl StateV2 {
    /// Clear the whole persisted state of the account, but keep just enough
    /// information to eventually reconnect to the same user account later.
    fn start_over(&self) -> StateV2 {
        StateV2 {
            config: self.config.clone(),
            current_device_id: None,
            // Leave the profile cache untouched so we can reconnect later.
            last_seen_profile: self.last_seen_profile.clone(),
            refresh_token: None,
            scoped_keys: HashMap::new(),
            last_handled_command: None,
            commands_data: HashMap::new(),
            access_token_cache: HashMap::new(),
            device_capabilities: HashSet::new(),
            session_token: None,
        }
    }
}

impl FirefoxAccount {
    fn from_state(state: StateV2) -> Self {
        Self {
            client: Arc::new(http_client::Client::new()),
            state,
            flow_store: HashMap::new(),
        }
    }

    /// Create a new `FirefoxAccount` instance using a `Config`.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn with_config(config: Config) -> Self {
        Self::from_state(StateV2 {
            config,
            refresh_token: None,
            scoped_keys: HashMap::new(),
            last_handled_command: None,
            commands_data: HashMap::new(),
            device_capabilities: HashSet::new(),
            session_token: None,
            current_device_id: None,
            last_seen_profile: None,
            access_token_cache: HashMap::new(),
        })
    }

    /// Create a new `FirefoxAccount` instance.
    ///
    /// * `content_url` - The Firefox Account content server URL.
    /// * `client_id` - The OAuth `client_id`.
    /// * `redirect_uri` - The OAuth `redirect_uri`.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn new(content_url: &str, client_id: &str, redirect_uri: &str) -> Self {
        let config = Config::new(content_url, client_id, redirect_uri);
        Self::with_config(config)
    }

    #[cfg(test)]
    #[allow(dead_code)] // FIXME
    pub(crate) fn set_client(&mut self, client: Arc<FxAClient>) {
        self.client = client;
    }

    /// Restore a `FirefoxAccount` instance from a serialized state
    /// created using `to_json`.
    pub fn from_json(data: &str) -> Result<Self> {
        let state = state_persistence::state_from_json(data)?;
        Ok(Self::from_state(state))
    }

    /// Serialize a `FirefoxAccount` instance internal state
    /// to be restored later using `from_json`.
    pub fn to_json(&self) -> Result<String> {
        state_persistence::state_to_json(&self.state)
    }

    /// Clear the whole persisted/cached state of the account, but keep just
    /// enough information to eventually reconnect to the same user account later.
    pub fn start_over(&mut self) {
        self.state = self.state.start_over();
        self.flow_store.clear();
    }

    /// Get the Sync Token Server endpoint URL.
    pub fn get_token_server_endpoint_url(&self) -> Result<Url> {
        self.state.config.token_server_endpoint_url()
    }

    /// Get the "connection succeeded" page URL.
    /// It is typically used to redirect the user after
    /// having intercepted the OAuth login-flow state/code
    /// redirection.
    pub fn get_connection_success_url(&self) -> Result<Url> {
        let mut url = self
            .state
            .config
            .content_url_path("connect_another_device")?;
        url.query_pairs_mut()
            .append_pair("showSuccessMessage", "true");
        Ok(url)
    }

    /// Get the "manage account" page URL.
    /// It is typically used in the application's account status UI,
    /// to link the user out to a webpage where they can manage
    /// all the details of their account.
    ///
    /// * `entrypoint` - Application-provided string identifying the UI touchpoint
    ///                  through which the page was accessed, for metrics purposes.
    pub fn get_manage_account_url(&mut self, entrypoint: &str) -> Result<Url> {
        let mut url = self.state.config.content_url_path("settings")?;
        url.query_pairs_mut().append_pair("entrypoint", entrypoint);
        self.add_account_identifiers_to_url(url)
    }

    /// Get the "manage devices" page URL.
    /// It is typically used in the application's account status UI,
    /// to link the user out to a webpage where they can manage
    /// the devices connected to their account.
    ///
    /// * `entrypoint` - Application-provided string identifying the UI touchpoint
    ///                  through which the page was accessed, for metrics purposes.
    pub fn get_manage_devices_url(&mut self, entrypoint: &str) -> Result<Url> {
        let mut url = self.state.config.content_url_path("settings/clients")?;
        url.query_pairs_mut().append_pair("entrypoint", entrypoint);
        self.add_account_identifiers_to_url(url)
    }

    fn add_account_identifiers_to_url(&mut self, mut url: Url) -> Result<Url> {
        let profile = self.get_profile(false)?;
        url.query_pairs_mut()
            .append_pair("uid", &profile.uid)
            .append_pair("email", &profile.email);
        Ok(url)
    }

    /// Handle any incoming push message payload coming from the Firefox Accounts
    /// servers that has been decrypted and authenticated by the Push crate.
    ///
    /// Due to iOS platform restrictions, a push notification must always show UI,
    /// and therefore we only retrieve 1 command per message.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn handle_push_message(&mut self, payload: &str) -> Result<Vec<AccountEvent>> {
        let payload = serde_json::from_str(payload)?;
        match payload {
            PushPayload::CommandReceived(CommandReceivedPushPayload { index, .. }) => {
                if cfg!(target_os = "ios") {
                    self.fetch_device_command(index).map(|cmd| vec![cmd])
                } else {
                    self.poll_device_commands()
                }
            }
            PushPayload::Unknown => {
                log::info!("Unknown Push command.");
                Ok(vec![])
            }
        }
    }

    fn get_refresh_token(&self) -> Result<&str> {
        match self.state.refresh_token {
            Some(ref token_info) => Ok(&token_info.token),
            None => Err(ErrorKind::NoRefreshToken.into()),
        }
    }

    /// Disconnect from the account and optionaly destroy our device record. This will
    /// leave the account object in a state where it can eventually reconnect to the same user.
    /// This is a "best effort" infallible method: e.g. if the network is unreachable,
    /// the device could still be in the FxA devices manager.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn disconnect(&mut self) {
        if let Some(ref refresh_token) = self.state.refresh_token {
            // Delete the current device (which deletes the refresh token), or
            // the refresh token directly if we don't have a device.
            let destroy_result = match self.get_current_device() {
                // If we get an error trying to fetch our device record we'll at least
                // still try to delete the refresh token itself.
                Ok(Some(device)) => {
                    self.client
                        .destroy_device(&self.state.config, &refresh_token.token, &device.id)
                }
                _ => self
                    .client
                    .destroy_refresh_token(&self.state.config, &refresh_token.token),
            };
            if let Err(e) = destroy_result {
                log::warn!("Error while destroying the device: {}", e);
            }
        }
        self.start_over();
    }
}

pub enum AccountEvent {
    // In the future: ProfileUpdated etc.
    TabReceived((Option<Device>, SendTabPayload)),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct CachedResponse<T> {
    response: T,
    cached_at: u64,
    etag: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "command", content = "data")]
pub enum PushPayload {
    #[serde(rename = "fxaccounts:command_received")]
    CommandReceived(CommandReceivedPushPayload),
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
pub struct CommandReceivedPushPayload {
    command: String,
    index: u64,
    sender: String,
    url: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_client::FxAClientMock;

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

    #[test]
    fn test_get_manage_account_url() {
        let mut fxa =
            FirefoxAccount::new("https://stable.dev.lcip.org", "12345678", "https://foo.bar");
        // No current user -> Error.
        match fxa.get_manage_account_url("test").unwrap_err().kind() {
            ErrorKind::NoCachedToken(_) => {}
            _ => panic!("error not NoCachedToken"),
        };
        // With current user -> expected Url.
        fxa.add_cached_profile("123", "test@example.com");
        let url = fxa.get_manage_account_url("test").unwrap().to_string();
        assert_eq!(
            url,
            "https://stable.dev.lcip.org/settings?entrypoint=test&uid=123&email=test%40example.com"
                .to_string()
        );
    }

    #[test]
    fn test_get_manage_devices_url() {
        let mut fxa =
            FirefoxAccount::new("https://stable.dev.lcip.org", "12345678", "https://foo.bar");
        // No current user -> Error.
        match fxa.get_manage_devices_url("test").unwrap_err().kind() {
            ErrorKind::NoCachedToken(_) => {}
            _ => panic!("error not NoCachedToken"),
        };
        // With current user -> expected Url.
        fxa.add_cached_profile("123", "test@example.com");
        let url = fxa.get_manage_devices_url("test").unwrap().to_string();
        assert_eq!(
            url,
            "https://stable.dev.lcip.org/settings/clients?entrypoint=test&uid=123&email=test%40example.com"
                .to_string()
        );
    }

    #[test]
    fn test_deserialize_push_message() {
        let json = "{\"version\":1,\"command\":\"fxaccounts:command_received\",\"data\":{\"command\":\"send-tab-recv\",\"index\":1,\"sender\":\"bobo\",\"url\":\"https://mozilla.org\"}}";
        let _: PushPayload = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_handle_push_message_unknown_command() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));
        let json = "{\"version\":1,\"command\":\"huh\"}";
        let events = fxa.handle_push_message(json).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_disconnect_no_refresh_token() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));

        fxa.add_cached_token(
            "profile",
            AccessTokenInfo {
                scope: "profile".to_string(),
                token: "profiletok".to_string(),
                key: None,
                expires_at: u64::max_value(),
            },
        );

        let client = FxAClientMock::new();
        fxa.set_client(Arc::new(client));

        assert!(!fxa.state.access_token_cache.is_empty());
        fxa.disconnect();
        assert!(fxa.state.access_token_cache.is_empty());
    }

    #[test]
    fn test_disconnect_device() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));

        fxa.state.refresh_token = Some(RefreshToken {
            token: "refreshtok".to_string(),
            scopes: HashSet::default(),
        });

        let mut client = FxAClientMock::new();
        client
            .expect_devices(mockiato::Argument::any, |token| {
                token.partial_eq("refreshtok")
            })
            .times(1)
            .returns_once(Ok(vec![
                Device {
                    common: http_client::DeviceResponseCommon {
                        id: "1234a".to_owned(),
                        display_name: "My Device".to_owned(),
                        device_type: http_client::DeviceType::Mobile,
                        push_subscription: None,
                        available_commands: HashMap::default(),
                        push_endpoint_expired: false,
                    },
                    is_current_device: true,
                    location: http_client::DeviceLocation {
                        city: None,
                        country: None,
                        state: None,
                        state_code: None,
                    },
                    last_access_time: None,
                },
                Device {
                    common: http_client::DeviceResponseCommon {
                        id: "a4321".to_owned(),
                        display_name: "My Other Device".to_owned(),
                        device_type: http_client::DeviceType::Desktop,
                        push_subscription: None,
                        available_commands: HashMap::default(),
                        push_endpoint_expired: false,
                    },
                    is_current_device: false,
                    location: http_client::DeviceLocation {
                        city: None,
                        country: None,
                        state: None,
                        state_code: None,
                    },
                    last_access_time: None,
                },
            ]));
        client
            .expect_destroy_device(
                mockiato::Argument::any,
                |token| token.partial_eq("refreshtok"),
                |device_id| device_id.partial_eq("1234a"),
            )
            .times(1)
            .returns_once(Ok(()));
        fxa.set_client(Arc::new(client));

        assert!(fxa.state.refresh_token.is_some());
        fxa.disconnect();
        assert!(fxa.state.refresh_token.is_none());
    }

    #[test]
    fn test_disconnect_no_device() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));

        fxa.state.refresh_token = Some(RefreshToken {
            token: "refreshtok".to_string(),
            scopes: HashSet::default(),
        });

        let mut client = FxAClientMock::new();
        client
            .expect_devices(mockiato::Argument::any, |token| {
                token.partial_eq("refreshtok")
            })
            .times(1)
            .returns_once(Ok(vec![Device {
                common: http_client::DeviceResponseCommon {
                    id: "a4321".to_owned(),
                    display_name: "My Other Device".to_owned(),
                    device_type: http_client::DeviceType::Desktop,
                    push_subscription: None,
                    available_commands: HashMap::default(),
                    push_endpoint_expired: false,
                },
                is_current_device: false,
                location: http_client::DeviceLocation {
                    city: None,
                    country: None,
                    state: None,
                    state_code: None,
                },
                last_access_time: None,
            }]));
        client
            .expect_destroy_refresh_token(mockiato::Argument::any, |token| {
                token.partial_eq("refreshtok")
            })
            .times(1)
            .returns_once(Ok(()));
        fxa.set_client(Arc::new(client));

        assert!(fxa.state.refresh_token.is_some());
        fxa.disconnect();
        assert!(fxa.state.refresh_token.is_none());
    }

    #[test]
    fn test_disconnect_network_errors() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));

        fxa.state.refresh_token = Some(RefreshToken {
            token: "refreshtok".to_string(),
            scopes: HashSet::default(),
        });

        let mut client = FxAClientMock::new();
        client
            .expect_devices(mockiato::Argument::any, |token| {
                token.partial_eq("refreshtok")
            })
            .times(1)
            .returns_once(Ok(vec![]));
        client
            .expect_destroy_refresh_token(mockiato::Argument::any, |token| {
                token.partial_eq("refreshtok")
            })
            .times(1)
            .returns_once(Err(ErrorKind::RemoteError {
                code: 500,
                errno: 101,
                error: "Did not work!".to_owned(),
                message: "Did not work!".to_owned(),
                info: "Did not work!".to_owned(),
            }
            .into()));
        fxa.set_client(Arc::new(client));

        assert!(fxa.state.refresh_token.is_some());
        fxa.disconnect();
        assert!(fxa.state.refresh_token.is_none());
    }
}

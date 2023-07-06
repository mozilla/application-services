/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Internal implementation details for the fxa_client crate.

use self::{
    config::Config,
    oauth::{AuthCircuitBreaker, OAuthFlow, OAUTH_WEBCHANNEL_REDIRECT},
    state_manager::StateManager,
    state_persistence::PersistedState,
    telemetry::FxaTelemetry,
};
use crate::{Error, FxaConfig, Result};
use serde_derive::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use url::Url;

#[cfg(feature = "integration_test")]
pub mod auth;
mod commands;
pub mod config;
pub mod device;
mod http_client;
mod oauth;
mod profile;
mod push;
mod scoped_keys;
mod scopes;
mod send_tab;
mod state_manager;
mod state_persistence;
mod telemetry;
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
    pub state: StateManager,
    attached_clients_cache: Option<CachedResponse<Vec<http_client::GetAttachedClientResponse>>>,
    devices_cache: Option<CachedResponse<Vec<http_client::GetDeviceResponse>>>,
    auth_circuit_breaker: AuthCircuitBreaker,
    telemetry: FxaTelemetry,
}

impl FirefoxAccount {
    fn from_state(state: PersistedState) -> Self {
        Self {
            client: Arc::new(http_client::Client::new()),
            state: StateManager::new(state),
            attached_clients_cache: None,
            devices_cache: None,
            auth_circuit_breaker: Default::default(),
            telemetry: FxaTelemetry::new(),
        }
    }

    /// Create a new `FirefoxAccount` instance using a `Config`.
    pub fn with_config(config: Config) -> Self {
        Self::from_state(PersistedState {
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
    pub fn new(config: FxaConfig) -> Self {
        Self::with_config(config.into())
    }

    #[cfg(test)]
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
        self.state.serialize_persisted_state()
    }

    /// Clear the attached clients and devices cache
    pub fn clear_devices_and_attached_clients_cache(&mut self) {
        self.attached_clients_cache = None;
        self.devices_cache = None;
    }

    /// Get the Sync Token Server endpoint URL.
    pub fn get_token_server_endpoint_url(&self) -> Result<String> {
        Ok(self.state.config().token_server_endpoint_url()?.into())
    }

    /// Get the pairing URL to navigate to on the Auth side (typically
    /// a computer).
    pub fn get_pairing_authority_url(&self) -> Result<String> {
        // Special case for the production server, we use the shorter firefox.com/pair URL.
        if self.state.config().content_url()? == Url::parse(config::CONTENT_URL_RELEASE)? {
            return Ok("https://firefox.com/pair".to_owned());
        }
        // Similarly special case for the China server.
        if self.state.config().content_url()? == Url::parse(config::CONTENT_URL_CHINA)? {
            return Ok("https://firefox.com.cn/pair".to_owned());
        }
        Ok(self.state.config().pair_url()?.into())
    }

    /// Get the "connection succeeded" page URL.
    /// It is typically used to redirect the user after
    /// having intercepted the OAuth login-flow state/code
    /// redirection.
    pub fn get_connection_success_url(&self) -> Result<String> {
        let mut url = self.state.config().connect_another_device_url()?;
        url.query_pairs_mut()
            .append_pair("showSuccessMessage", "true");
        Ok(url.into())
    }

    /// Get the "manage account" page URL.
    /// It is typically used in the application's account status UI,
    /// to link the user out to a webpage where they can manage
    /// all the details of their account.
    ///
    /// * `entrypoint` - Application-provided string identifying the UI touchpoint
    ///                  through which the page was accessed, for metrics purposes.
    pub fn get_manage_account_url(&mut self, entrypoint: &str) -> Result<String> {
        let mut url = self.state.config().settings_url()?;
        url.query_pairs_mut().append_pair("entrypoint", entrypoint);
        if self.state.config().redirect_uri == OAUTH_WEBCHANNEL_REDIRECT {
            url.query_pairs_mut()
                .append_pair("context", "oauth_webchannel_v1");
        }
        self.add_account_identifiers_to_url(url)
    }

    /// Get the "manage devices" page URL.
    /// It is typically used in the application's account status UI,
    /// to link the user out to a webpage where they can manage
    /// the devices connected to their account.
    ///
    /// * `entrypoint` - Application-provided string identifying the UI touchpoint
    ///                  through which the page was accessed, for metrics purposes.
    pub fn get_manage_devices_url(&mut self, entrypoint: &str) -> Result<String> {
        let mut url = self.state.config().settings_clients_url()?;
        url.query_pairs_mut().append_pair("entrypoint", entrypoint);
        self.add_account_identifiers_to_url(url)
    }

    fn add_account_identifiers_to_url(&mut self, mut url: Url) -> Result<String> {
        let profile = self.get_profile(false)?;
        url.query_pairs_mut()
            .append_pair("uid", &profile.uid)
            .append_pair("email", &profile.email);
        Ok(url.into())
    }

    fn get_refresh_token(&self) -> Result<&str> {
        match self.state.refresh_token() {
            Some(token_info) => Ok(&token_info.token),
            None => Err(Error::NoRefreshToken),
        }
    }

    /// Disconnect from the account and optionally destroy our device record. This will
    /// leave the account object in a state where it can eventually reconnect to the same user.
    /// This is a "best effort" infallible method: e.g. if the network is unreachable,
    /// the device could still be in the FxA devices manager.
    pub fn disconnect(&mut self) {
        let current_device_result;
        {
            current_device_result = self.get_current_device();
        }

        if let Some(refresh_token) = self.state.refresh_token() {
            // Delete the current device (which deletes the refresh token), or
            // the refresh token directly if we don't have a device.
            let destroy_result = match current_device_result {
                // If we get an error trying to fetch our device record we'll at least
                // still try to delete the refresh token itself.
                Ok(Some(device)) => self.client.destroy_device_record(
                    self.state.config(),
                    &refresh_token.token,
                    &device.id,
                ),
                _ => self
                    .client
                    .destroy_refresh_token(self.state.config(), &refresh_token.token),
            };
            if let Err(e) = destroy_result {
                log::warn!("Error while destroying the device: {}", e);
            }
        }
        self.state.disconnect();
        self.clear_devices_and_attached_clients_cache();
        self.telemetry = FxaTelemetry::new();
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct CachedResponse<T> {
    response: T,
    cached_at: u64,
    etag: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::device::*;
    use crate::internal::http_client::FxAClientMock;
    use crate::internal::oauth::*;

    #[test]
    fn test_fxa_is_send() {
        fn is_send<T: Send>() {}
        is_send::<FirefoxAccount>();
    }

    #[test]
    fn test_serialize_deserialize() {
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let fxa1 = FirefoxAccount::with_config(config);
        let fxa1_json = fxa1.to_json().unwrap();
        drop(fxa1);
        let fxa2 = FirefoxAccount::from_json(&fxa1_json).unwrap();
        let fxa2_json = fxa2.to_json().unwrap();
        assert_eq!(fxa1_json, fxa2_json);
    }

    #[test]
    fn test_get_connection_success_url() {
        let config = Config::new("https://stable.dev.lcip.org", "12345678", "https://foo.bar");
        let fxa = FirefoxAccount::with_config(config);
        let url = fxa.get_connection_success_url().unwrap();
        assert_eq!(
            url,
            "https://stable.dev.lcip.org/connect_another_device?showSuccessMessage=true"
                .to_string()
        );
    }

    #[test]
    fn test_get_manage_account_url() {
        let config = Config::new("https://stable.dev.lcip.org", "12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);
        // No current user -> Error.
        match fxa.get_manage_account_url("test").unwrap_err() {
            Error::NoCachedToken(_) => {}
            _ => panic!("error not NoCachedToken"),
        };
        // With current user -> expected Url.
        fxa.add_cached_profile("123", "test@example.com");
        let url = fxa.get_manage_account_url("test").unwrap();
        assert_eq!(
            url,
            "https://stable.dev.lcip.org/settings?entrypoint=test&uid=123&email=test%40example.com"
                .to_string()
        );
    }

    #[test]
    fn test_get_manage_account_url_with_webchannel_redirect() {
        let config = Config::new(
            "https://stable.dev.lcip.org",
            "12345678",
            OAUTH_WEBCHANNEL_REDIRECT,
        );
        let mut fxa = FirefoxAccount::with_config(config);
        fxa.add_cached_profile("123", "test@example.com");
        let url = fxa.get_manage_account_url("test").unwrap();
        assert_eq!(
            url,
            "https://stable.dev.lcip.org/settings?entrypoint=test&context=oauth_webchannel_v1&uid=123&email=test%40example.com"
                .to_string()
        );
    }

    #[test]
    fn test_get_manage_devices_url() {
        let config = Config::new("https://stable.dev.lcip.org", "12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);
        // No current user -> Error.
        match fxa.get_manage_devices_url("test").unwrap_err() {
            Error::NoCachedToken(_) => {}
            _ => panic!("error not NoCachedToken"),
        };
        // With current user -> expected Url.
        fxa.add_cached_profile("123", "test@example.com");
        let url = fxa.get_manage_devices_url("test").unwrap();
        assert_eq!(
            url,
            "https://stable.dev.lcip.org/settings/clients?entrypoint=test&uid=123&email=test%40example.com"
                .to_string()
        );
    }

    #[test]
    fn test_disconnect_no_refresh_token() {
        let config = Config::new("https://stable.dev.lcip.org", "12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);

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

        assert!(!fxa.state.is_access_token_cache_empty());
        fxa.disconnect();
        assert!(fxa.state.is_access_token_cache_empty());
    }

    #[test]
    fn test_disconnect_device() {
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);

        fxa.state.force_refresh_token(RefreshToken {
            token: "refreshtok".to_string(),
            scopes: HashSet::default(),
        });

        let mut client = FxAClientMock::new();
        client
            .expect_get_devices(mockiato::Argument::any, |token| {
                token.partial_eq("refreshtok")
            })
            .times(1)
            .returns_once(Ok(vec![
                GetDeviceResponse {
                    common: http_client::DeviceResponseCommon {
                        id: "1234a".to_owned(),
                        display_name: "My Device".to_owned(),
                        device_type: sync15::DeviceType::Mobile,
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
                GetDeviceResponse {
                    common: http_client::DeviceResponseCommon {
                        id: "a4321".to_owned(),
                        display_name: "My Other Device".to_owned(),
                        device_type: sync15::DeviceType::Desktop,
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
            .expect_destroy_device_record(
                mockiato::Argument::any,
                |token| token.partial_eq("refreshtok"),
                |device_id| device_id.partial_eq("1234a"),
            )
            .times(1)
            .returns_once(Ok(()));
        fxa.set_client(Arc::new(client));

        assert!(fxa.state.refresh_token().is_some());
        fxa.disconnect();
        assert!(fxa.state.refresh_token().is_none());
    }

    #[test]
    fn test_disconnect_no_device() {
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);

        fxa.state.force_refresh_token(RefreshToken {
            token: "refreshtok".to_string(),
            scopes: HashSet::default(),
        });

        let mut client = FxAClientMock::new();
        client
            .expect_get_devices(mockiato::Argument::any, |token| {
                token.partial_eq("refreshtok")
            })
            .times(1)
            .returns_once(Ok(vec![GetDeviceResponse {
                common: http_client::DeviceResponseCommon {
                    id: "a4321".to_owned(),
                    display_name: "My Other Device".to_owned(),
                    device_type: sync15::DeviceType::Desktop,
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

        assert!(fxa.state.refresh_token().is_some());
        fxa.disconnect();
        assert!(fxa.state.refresh_token().is_none());
    }

    #[test]
    fn test_disconnect_network_errors() {
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);

        fxa.state.force_refresh_token(RefreshToken {
            token: "refreshtok".to_string(),
            scopes: HashSet::default(),
        });

        let mut client = FxAClientMock::new();
        client
            .expect_get_devices(mockiato::Argument::any, |token| {
                token.partial_eq("refreshtok")
            })
            .times(1)
            .returns_once(Ok(vec![]));
        client
            .expect_destroy_refresh_token(mockiato::Argument::any, |token| {
                token.partial_eq("refreshtok")
            })
            .times(1)
            .returns_once(Err(Error::RemoteError {
                code: 500,
                errno: 101,
                error: "Did not work!".to_owned(),
                message: "Did not work!".to_owned(),
                info: "Did not work!".to_owned(),
            }));
        fxa.set_client(Arc::new(client));

        assert!(fxa.state.refresh_token().is_some());
        fxa.disconnect();
        assert!(fxa.state.refresh_token().is_none());
    }

    #[test]
    fn test_get_pairing_authority_url() {
        let config = Config::new("https://foo.bar", "12345678", "https://foo.bar");
        let fxa = FirefoxAccount::with_config(config);
        assert_eq!(
            fxa.get_pairing_authority_url().unwrap().as_str(),
            "https://foo.bar/pair"
        );

        let config = Config::release("12345678", "https://foo.bar");
        let fxa = FirefoxAccount::with_config(config);
        assert_eq!(
            fxa.get_pairing_authority_url().unwrap().as_str(),
            "https://firefox.com/pair"
        );

        let config = Config::china("12345678", "https://foo.bar");
        let fxa = FirefoxAccount::with_config(config);
        assert_eq!(
            fxa.get_pairing_authority_url().unwrap().as_str(),
            "https://firefox.com.cn/pair"
        )
    }
}

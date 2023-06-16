/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Low-level API for talking to the FxA server.
//!
//! This module is responsible for talking to the FxA server over HTTP,
//! serializing request bodies and deserializing response payloads into
//! live objects that can be inspected by other parts of the code.

use super::{config::Config, util};
use crate::{Error, Result};
use parking_lot::Mutex;
use rc_crypto::{
    digest,
    hawk::{Credentials, Key, PayloadHasher, RequestBuilder, SHA256},
    hkdf, hmac,
};
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use sync15::DeviceType;
use url::Url;
use viaduct::{header_names, status_codes, Method, Request, Response};

const HAWK_HKDF_SALT: [u8; 32] = [0b0; 32];
const HAWK_KEY_LENGTH: usize = 32;
const RETRY_AFTER_DEFAULT_SECONDS: u64 = 10;
// Devices older than this many days will not appear in the devices list
const DEVICES_FILTER_DAYS: u64 = 21;

/// Trait defining the low-level API for talking to the FxA server.
///
/// These are all the methods and datatypes used for interacting with
/// the FxA server. It's defined as a trait mostly to make it mockable
/// for testing purposes.
///
/// The default live implementation of this trait can be found in
/// the [`Client`] struct, and you should consult that struct for
/// documentation on specific methods.
///
/// A brief note on names: you'll see that the method names on this trait
/// try to follow a pattern of `<verb>_<noun>[_<additional info>]()`.
/// Please try to keep to this pattern if you add methods to this trait.
///
/// Consistent names are helpful at the best of times, but they're
/// particularly important here because so many of the nouns in OAuth
/// contain embedded verbs! For example: at a glance, does a method
/// named `refresh_token` refresh something called a "token", or does
/// it return something called a "refresh token"? Using unambiguous
/// verbs to start each method helps avoid confusion here.
///
#[cfg_attr(test, mockiato::mockable)]
pub(crate) trait FxAClient {
    fn create_refresh_token_using_authorization_code(
        &self,
        config: &Config,
        code: &str,
        code_verifier: &str,
    ) -> Result<OAuthTokenResponse>;
    fn create_refresh_token_using_session_token(
        &self,
        config: &Config,
        session_token: &str,
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse>;
    fn check_refresh_token_status(
        &self,
        config: &Config,
        refresh_token: &str,
    ) -> Result<IntrospectResponse>;
    fn create_access_token_using_refresh_token(
        &self,
        config: &Config,
        refresh_token: &str,
        ttl: Option<u64>,
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse>;
    fn create_access_token_using_session_token(
        &self,
        config: &Config,
        session_token: &str,
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse>;
    fn create_authorization_code_using_session_token(
        &self,
        config: &Config,
        session_token: &str,
        auth_params: AuthorizationRequestParameters,
    ) -> Result<OAuthAuthResponse>;
    fn duplicate_session_token(
        &self,
        config: &Config,
        session_token: &str,
    ) -> Result<DuplicateTokenResponse>;
    fn destroy_access_token(&self, config: &Config, token: &str) -> Result<()>;
    fn destroy_refresh_token(&self, config: &Config, token: &str) -> Result<()>;
    fn get_profile(
        &self,
        config: &Config,
        profile_access_token: &str,
        etag: Option<String>,
    ) -> Result<Option<ResponseAndETag<ProfileResponse>>>;
    fn get_pending_commands(
        &self,
        config: &Config,
        refresh_token: &str,
        index: u64,
        limit: Option<u64>,
    ) -> Result<PendingCommandsResponse>;
    fn invoke_command(
        &self,
        config: &Config,
        refresh_token: &str,
        command: &str,
        target: &str,
        payload: &serde_json::Value,
    ) -> Result<()>;
    fn update_device_record(
        &self,
        config: &Config,
        refresh_token: &str,
        update: DeviceUpdateRequest<'_>,
    ) -> Result<UpdateDeviceResponse>;
    fn destroy_device_record(&self, config: &Config, refresh_token: &str, id: &str) -> Result<()>;
    fn get_devices(&self, config: &Config, refresh_token: &str) -> Result<Vec<GetDeviceResponse>>;
    fn get_attached_clients(
        &self,
        config: &Config,
        session_token: &str,
    ) -> Result<Vec<GetAttachedClientResponse>>;
    fn get_scoped_key_data(
        &self,
        config: &Config,
        session_token: &str,
        client_id: &str,
        scope: &str,
    ) -> Result<HashMap<String, ScopedKeyDataResponse>>;
    fn get_fxa_client_configuration(&self, config: &Config) -> Result<ClientConfigurationResponse>;
    fn get_openid_configuration(&self, config: &Config) -> Result<OpenIdConfigurationResponse>;
}

enum HttpClientState {
    Ok,
    Backoff {
        backoff_end_duration: Duration,
        time_since_backoff: Instant,
    },
}

pub struct Client {
    state: Mutex<HashMap<String, HttpClientState>>,
}
impl FxAClient for Client {
    fn get_fxa_client_configuration(&self, config: &Config) -> Result<ClientConfigurationResponse> {
        // Why go through two-levels of indirection? It looks kinda dumb.
        // Well, `config:Config` also needs to fetch the config, but does not have access
        // to an instance of `http_client`, so it calls the helper function directly.
        fxa_client_configuration(config.client_config_url()?)
    }
    fn get_openid_configuration(&self, config: &Config) -> Result<OpenIdConfigurationResponse> {
        openid_configuration(config.openid_config_url()?)
    }

    fn get_profile(
        &self,
        config: &Config,
        access_token: &str,
        etag: Option<String>,
    ) -> Result<Option<ResponseAndETag<ProfileResponse>>> {
        let url = config.userinfo_endpoint()?;
        let mut request =
            Request::get(url).header(header_names::AUTHORIZATION, bearer_token(access_token))?;
        if let Some(etag) = etag {
            request = request.header(header_names::IF_NONE_MATCH, format!("\"{}\"", etag))?;
        }
        let resp = self.make_request(request)?;
        if resp.status == status_codes::NOT_MODIFIED {
            return Ok(None);
        }
        let etag = resp
            .headers
            .get(header_names::ETAG)
            .map(ToString::to_string);
        Ok(Some(ResponseAndETag {
            etag,
            response: resp.json()?,
        }))
    }

    fn create_refresh_token_using_authorization_code(
        &self,
        config: &Config,
        code: &str,
        code_verifier: &str,
    ) -> Result<OAuthTokenResponse> {
        let req_body = OAauthTokenRequest::UsingCode {
            code: code.to_string(),
            client_id: config.client_id.to_string(),
            code_verifier: code_verifier.to_string(),
            ttl: None,
        };
        self.make_oauth_token_request(config, serde_json::to_value(req_body).unwrap())
    }

    fn create_refresh_token_using_session_token(
        &self,
        config: &Config,
        session_token: &str,
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse> {
        let url = config.token_endpoint()?;
        let key = derive_auth_key_from_session_token(session_token)?;
        let body = json!({
            "client_id": config.client_id,
            "scope": scopes.join(" "),
            "grant_type": "fxa-credentials",
            "access_type": "offline",
        });
        let request = HawkRequestBuilder::new(Method::Post, url, &key)
            .body(body)
            .build()?;
        Ok(self.make_request(request)?.json()?)
    }

    // For the regular generation of an `access_token` from long-lived credentials.

    fn create_access_token_using_refresh_token(
        &self,
        config: &Config,
        refresh_token: &str,
        ttl: Option<u64>,
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse> {
        let req = OAauthTokenRequest::UsingRefreshToken {
            client_id: config.client_id.clone(),
            refresh_token: refresh_token.to_string(),
            scope: Some(scopes.join(" ")),
            ttl,
        };
        self.make_oauth_token_request(config, serde_json::to_value(req).unwrap())
    }

    fn create_access_token_using_session_token(
        &self,
        config: &Config,
        session_token: &str,
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse> {
        let parameters = json!({
            "client_id": config.client_id,
            "grant_type": "fxa-credentials",
            "scope": scopes.join(" ")
        });
        let key = derive_auth_key_from_session_token(session_token)?;
        let url = config.token_endpoint()?;
        let request = HawkRequestBuilder::new(Method::Post, url, &key)
            .body(parameters)
            .build()?;
        self.make_request(request)?.json().map_err(Into::into)
    }

    fn create_authorization_code_using_session_token(
        &self,
        config: &Config,
        session_token: &str,
        auth_params: AuthorizationRequestParameters,
    ) -> Result<OAuthAuthResponse> {
        let parameters = serde_json::to_value(auth_params)?;
        let key = derive_auth_key_from_session_token(session_token)?;
        let url = config.auth_url_path("v1/oauth/authorization")?;
        let request = HawkRequestBuilder::new(Method::Post, url, &key)
            .body(parameters)
            .build()?;

        Ok(self.make_request(request)?.json()?)
    }

    fn check_refresh_token_status(
        &self,
        config: &Config,
        refresh_token: &str,
    ) -> Result<IntrospectResponse> {
        let body = json!({
            "token_type_hint": "refresh_token",
            "token": refresh_token,
        });
        let url = config.introspection_endpoint()?;
        Ok(self.make_request(Request::post(url).json(&body))?.json()?)
    }

    fn duplicate_session_token(
        &self,
        config: &Config,
        session_token: &str,
    ) -> Result<DuplicateTokenResponse> {
        let url = config.auth_url_path("v1/session/duplicate")?;
        let key = derive_auth_key_from_session_token(session_token)?;
        let duplicate_body = json!({
            "reason": "migration"
        });
        let request = HawkRequestBuilder::new(Method::Post, url, &key)
            .body(duplicate_body)
            .build()?;

        Ok(self.make_request(request)?.json()?)
    }

    fn destroy_access_token(&self, config: &Config, access_token: &str) -> Result<()> {
        let body = json!({
            "token": access_token,
        });
        self.destroy_token_helper(config, &body)
    }

    fn destroy_refresh_token(&self, config: &Config, refresh_token: &str) -> Result<()> {
        let body = json!({
            "refresh_token": refresh_token,
        });
        self.destroy_token_helper(config, &body)
    }

    fn get_pending_commands(
        &self,
        config: &Config,
        refresh_token: &str,
        index: u64,
        limit: Option<u64>,
    ) -> Result<PendingCommandsResponse> {
        let url = config.auth_url_path("v1/account/device/commands")?;
        let mut request = Request::get(url)
            .header(header_names::AUTHORIZATION, bearer_token(refresh_token))?
            .query(&[("index", &index.to_string())]);
        if let Some(limit) = limit {
            request = request.query(&[("limit", &limit.to_string())])
        }
        Ok(self.make_request(request)?.json()?)
    }

    fn invoke_command(
        &self,
        config: &Config,
        refresh_token: &str,
        command: &str,
        target: &str,
        payload: &serde_json::Value,
    ) -> Result<()> {
        let body = json!({
            "command": command,
            "target": target,
            "payload": payload
        });
        let url = config.auth_url_path("v1/account/devices/invoke_command")?;
        let request = Request::post(url)
            .header(header_names::AUTHORIZATION, bearer_token(refresh_token))?
            .header(header_names::CONTENT_TYPE, "application/json")?
            .body(body.to_string());
        self.make_request(request)?;
        Ok(())
    }

    fn get_devices(&self, config: &Config, refresh_token: &str) -> Result<Vec<GetDeviceResponse>> {
        let url = config.auth_url_path("v1/account/devices")?;
        let request = Request::get(url)
            .header(header_names::AUTHORIZATION, bearer_token(refresh_token))?
            .query(&[(
                "filterIdleDevicesTimestamp",
                &util::past_timestamp(DEVICES_FILTER_DAYS).to_string(),
            )]);
        Ok(self.make_request(request)?.json()?)
    }

    fn update_device_record(
        &self,
        config: &Config,
        refresh_token: &str,
        update: DeviceUpdateRequest<'_>,
    ) -> Result<UpdateDeviceResponse> {
        let url = config.auth_url_path("v1/account/device")?;
        let request = Request::post(url)
            .header(header_names::AUTHORIZATION, bearer_token(refresh_token))?
            .header(header_names::CONTENT_TYPE, "application/json")?
            .body(serde_json::to_string(&update)?);
        Ok(self.make_request(request)?.json()?)
    }

    fn destroy_device_record(&self, config: &Config, refresh_token: &str, id: &str) -> Result<()> {
        let body = json!({
            "id": id,
        });
        let url = config.auth_url_path("v1/account/device/destroy")?;
        let request = Request::post(url)
            .header(header_names::AUTHORIZATION, bearer_token(refresh_token))?
            .header(header_names::CONTENT_TYPE, "application/json")?
            .body(body.to_string());

        self.make_request(request)?;
        Ok(())
    }

    fn get_attached_clients(
        &self,
        config: &Config,
        session_token: &str,
    ) -> Result<Vec<GetAttachedClientResponse>> {
        let url = config.auth_url_path("v1/account/attached_clients")?;
        let key = derive_auth_key_from_session_token(session_token)?;
        let request = HawkRequestBuilder::new(Method::Get, url, &key).build()?;
        Ok(self.make_request(request)?.json()?)
    }

    fn get_scoped_key_data(
        &self,
        config: &Config,
        session_token: &str,
        client_id: &str,
        scope: &str,
    ) -> Result<HashMap<String, ScopedKeyDataResponse>> {
        let body = json!({
            "client_id": client_id,
            "scope": scope,
        });
        let url = config.auth_url_path("v1/account/scoped-key-data")?;
        let key = derive_auth_key_from_session_token(session_token)?;
        let request = HawkRequestBuilder::new(Method::Post, url, &key)
            .body(body)
            .build()?;
        self.make_request(request)?.json().map_err(|e| e.into())
    }
}

macro_rules! fetch {
    ($url:expr) => {
        viaduct::Request::get($url)
            .send()?
            .require_success()?
            .json()?
    };
}

#[inline]
pub(crate) fn fxa_client_configuration(url: Url) -> Result<ClientConfigurationResponse> {
    Ok(fetch!(url))
}
#[inline]
pub(crate) fn openid_configuration(url: Url) -> Result<OpenIdConfigurationResponse> {
    Ok(fetch!(url))
}

impl Client {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
        }
    }

    fn destroy_token_helper(&self, config: &Config, body: &serde_json::Value) -> Result<()> {
        let url = config.oauth_url_path("v1/destroy")?;
        self.make_request(Request::post(url).json(body))?;
        Ok(())
    }

    fn make_oauth_token_request(
        &self,
        config: &Config,
        body: serde_json::Value,
    ) -> Result<OAuthTokenResponse> {
        let url = config.token_endpoint()?;
        Ok(self.make_request(Request::post(url).json(&body))?.json()?)
    }

    fn handle_too_many_requests(&self, resp: Response) -> Result<Response> {
        let path = resp.url.path().to_string();
        if let Some(retry_after) = resp.headers.get_as::<u64, _>(header_names::RETRY_AFTER) {
            let retry_after = retry_after.unwrap_or(RETRY_AFTER_DEFAULT_SECONDS);
            let time_out_state = HttpClientState::Backoff {
                backoff_end_duration: Duration::from_secs(retry_after),
                time_since_backoff: Instant::now(),
            };
            self.state.lock().insert(path, time_out_state);
            return Err(Error::BackoffError(retry_after));
        }
        Self::default_handle_response_error(resp)
    }

    fn default_handle_response_error(resp: Response) -> Result<Response> {
        let json: std::result::Result<serde_json::Value, _> = resp.json();
        match json {
            Ok(json) => Err(Error::RemoteError {
                code: json["code"].as_u64().unwrap_or(0),
                errno: json["errno"].as_u64().unwrap_or(0),
                error: json["error"].as_str().unwrap_or("").to_string(),
                message: json["message"].as_str().unwrap_or("").to_string(),
                info: json["info"].as_str().unwrap_or("").to_string(),
            }),
            Err(_) => Err(resp.require_success().unwrap_err().into()),
        }
    }

    fn make_request(&self, request: Request) -> Result<Response> {
        let url = request.url.path().to_string();
        if let HttpClientState::Backoff {
            backoff_end_duration,
            time_since_backoff,
        } = self.state.lock().get(&url).unwrap_or(&HttpClientState::Ok)
        {
            let elapsed_time = time_since_backoff.elapsed();
            if elapsed_time < *backoff_end_duration {
                let remaining = *backoff_end_duration - elapsed_time;
                return Err(Error::BackoffError(remaining.as_secs()));
            }
        }
        self.state.lock().insert(url, HttpClientState::Ok);
        let resp = request.send()?;
        if resp.is_success() || resp.status == status_codes::NOT_MODIFIED {
            Ok(resp)
        } else {
            match resp.status {
                status_codes::TOO_MANY_REQUESTS => self.handle_too_many_requests(resp),
                _ => Self::default_handle_response_error(resp),
            }
        }
    }
}

fn bearer_token(token: &str) -> String {
    format!("Bearer {}", token)
}

fn kw(name: &str) -> Vec<u8> {
    format!("identity.mozilla.com/picl/v1/{}", name)
        .as_bytes()
        .to_vec()
}

pub fn derive_auth_key_from_session_token(session_token: &str) -> Result<Vec<u8>> {
    let session_token_bytes = hex::decode(session_token)?;
    let context_info = kw("sessionToken");
    let salt = hmac::SigningKey::new(&digest::SHA256, &HAWK_HKDF_SALT);
    let mut out = vec![0u8; HAWK_KEY_LENGTH * 2];
    hkdf::extract_and_expand(&salt, &session_token_bytes, &context_info, &mut out)?;
    Ok(out)
}

#[derive(Serialize, Deserialize)]
pub struct AuthorizationRequestParameters {
    pub client_id: String,
    pub scope: String,
    pub state: String,
    pub access_type: String,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub keys_jwe: Option<String>,
}

struct HawkRequestBuilder<'a> {
    url: Url,
    method: Method,
    body: Option<String>,
    hkdf_sha256_key: &'a [u8],
}

impl<'a> HawkRequestBuilder<'a> {
    pub fn new(method: Method, url: Url, hkdf_sha256_key: &'a [u8]) -> Self {
        rc_crypto::ensure_initialized();
        HawkRequestBuilder {
            url,
            method,
            body: None,
            hkdf_sha256_key,
        }
    }

    // This class assumes that the content being sent it always of the type
    // application/json.
    pub fn body(mut self, body: serde_json::Value) -> Self {
        self.body = Some(body.to_string());
        self
    }

    fn make_hawk_header(&self) -> Result<String> {
        // Make sure we de-allocate the hash after hawk_request_builder.
        let hash;
        let method = format!("{}", self.method);
        let mut hawk_request_builder = RequestBuilder::from_url(method.as_str(), &self.url)?;
        if let Some(ref body) = self.body {
            hash = PayloadHasher::hash("application/json", SHA256, body)?;
            hawk_request_builder = hawk_request_builder.hash(&hash[..]);
        }
        let hawk_request = hawk_request_builder.request();
        let token_id = hex::encode(&self.hkdf_sha256_key[0..HAWK_KEY_LENGTH]);
        let hmac_key = &self.hkdf_sha256_key[HAWK_KEY_LENGTH..(2 * HAWK_KEY_LENGTH)];
        let hawk_credentials = Credentials {
            id: token_id,
            key: Key::new(hmac_key, SHA256)?,
        };
        let header = hawk_request.make_header(&hawk_credentials)?;
        Ok(format!("Hawk {}", header))
    }

    pub fn build(self) -> Result<Request> {
        let hawk_header = self.make_hawk_header()?;
        let mut request =
            Request::new(self.method, self.url).header(header_names::AUTHORIZATION, hawk_header)?;
        if let Some(body) = self.body {
            request = request
                .header(header_names::CONTENT_TYPE, "application/json")?
                .body(body);
        }
        Ok(request)
    }
}

#[derive(Deserialize)]
pub(crate) struct ClientConfigurationResponse {
    pub(crate) auth_server_base_url: String,
    pub(crate) oauth_server_base_url: String,
    pub(crate) profile_server_base_url: String,
    pub(crate) sync_tokenserver_base_url: String,
}

#[derive(Deserialize)]
pub(crate) struct OpenIdConfigurationResponse {
    pub(crate) authorization_endpoint: String,
    pub(crate) introspection_endpoint: String,
    pub(crate) issuer: String,
    pub(crate) jwks_uri: String,
    #[allow(dead_code)]
    pub(crate) token_endpoint: String,
    pub(crate) userinfo_endpoint: String,
}

#[derive(Clone)]
pub struct ResponseAndETag<T> {
    pub response: T,
    pub etag: Option<String>,
}

#[derive(Deserialize)]
pub struct PendingCommandsResponse {
    pub index: u64,
    pub last: Option<bool>,
    pub messages: Vec<PendingCommand>,
}

#[derive(Deserialize)]
pub struct PendingCommand {
    pub index: u64,
    pub data: CommandData,
}

#[derive(Debug, Deserialize)]
pub struct CommandData {
    pub command: String,
    pub payload: serde_json::Value, // Need https://github.com/serde-rs/serde/issues/912 to make payload an enum instead.
    pub sender: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PushSubscription {
    #[serde(rename = "pushCallback")]
    pub endpoint: String,
    #[serde(rename = "pushPublicKey")]
    pub public_key: String,
    #[serde(rename = "pushAuthKey")]
    pub auth_key: String,
}

impl From<crate::DevicePushSubscription> for PushSubscription {
    fn from(sub: crate::DevicePushSubscription) -> Self {
        PushSubscription {
            endpoint: sub.endpoint,
            public_key: sub.public_key,
            auth_key: sub.auth_key,
        }
    }
}

impl From<PushSubscription> for crate::DevicePushSubscription {
    fn from(sub: PushSubscription) -> Self {
        crate::DevicePushSubscription {
            endpoint: sub.endpoint,
            public_key: sub.public_key,
            auth_key: sub.auth_key,
        }
    }
}

/// We use the double Option pattern in this struct.
/// The outer option represents the existence of the field
/// and the inner option its value or null.
/// TL;DR:
/// `None`: the field will not be present in the JSON body.
/// `Some(None)`: the field will have a `null` value.
/// `Some(Some(T))`: the field will have the serialized value of T.
#[derive(Serialize)]
#[allow(clippy::option_option)]
pub struct DeviceUpdateRequest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "name")]
    display_name: Option<Option<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    device_type: Option<&'a DeviceType>,
    #[serde(flatten)]
    push_subscription: Option<&'a PushSubscription>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "availableCommands")]
    available_commands: Option<Option<&'a HashMap<String, String>>>,
}

#[allow(clippy::option_option)]
pub struct DeviceUpdateRequestBuilder<'a> {
    device_type: Option<&'a DeviceType>,
    display_name: Option<Option<&'a str>>,
    push_subscription: Option<&'a PushSubscription>,
    available_commands: Option<Option<&'a HashMap<String, String>>>,
}

impl<'a> DeviceUpdateRequestBuilder<'a> {
    pub fn new() -> Self {
        Self {
            device_type: None,
            display_name: None,
            push_subscription: None,
            available_commands: None,
        }
    }

    pub fn push_subscription(mut self, push_subscription: &'a PushSubscription) -> Self {
        self.push_subscription = Some(push_subscription);
        self
    }

    pub fn available_commands(mut self, available_commands: &'a HashMap<String, String>) -> Self {
        self.available_commands = Some(Some(available_commands));
        self
    }

    pub fn clear_available_commands(mut self) -> Self {
        self.available_commands = Some(None);
        self
    }

    pub fn display_name(mut self, display_name: &'a str) -> Self {
        self.display_name = Some(Some(display_name));
        self
    }

    pub fn clear_display_name(mut self) -> Self {
        self.display_name = Some(None);
        self
    }

    pub fn device_type(mut self, device_type: &'a DeviceType) -> Self {
        self.device_type = Some(device_type);
        self
    }

    pub fn build(self) -> DeviceUpdateRequest<'a> {
        DeviceUpdateRequest {
            display_name: self.display_name,
            device_type: self.device_type,
            push_subscription: self.push_subscription,
            available_commands: self.available_commands,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceLocation {
    pub city: Option<String>,
    pub country: Option<String>,
    pub state: Option<String>,
    #[serde(rename = "stateCode")]
    pub state_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetDeviceResponse {
    #[serde(flatten)]
    pub common: DeviceResponseCommon,
    #[serde(rename = "isCurrentDevice")]
    pub is_current_device: bool,
    pub location: DeviceLocation,
    #[serde(rename = "lastAccessTime")]
    pub last_access_time: Option<u64>,
}

impl std::ops::Deref for GetDeviceResponse {
    type Target = DeviceResponseCommon;
    fn deref(&self) -> &DeviceResponseCommon {
        &self.common
    }
}

pub type UpdateDeviceResponse = DeviceResponseCommon;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceResponseCommon {
    pub id: String,
    #[serde(rename = "name")]
    pub display_name: String,
    #[serde(rename = "type")]
    pub device_type: DeviceType,
    #[serde(flatten)]
    pub push_subscription: Option<PushSubscription>,
    #[serde(rename = "availableCommands")]
    pub available_commands: HashMap<String, String>,
    #[serde(rename = "pushEndpointExpired")]
    pub push_endpoint_expired: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAttachedClientResponse {
    pub client_id: Option<String>,
    pub session_token_id: Option<String>,
    pub refresh_token_id: Option<String>,
    pub device_id: Option<String>,
    pub device_type: DeviceType,
    pub is_current_session: bool,
    pub name: Option<String>,
    pub created_time: Option<u64>,
    pub last_access_time: Option<u64>,
    pub scope: Option<Vec<String>>,
    pub user_agent: String,
    pub os: Option<String>,
}

// We model the OAuthTokenRequest according to the up to date
// definition on
// https://github.com/mozilla/fxa/blob/8ae0e6876a50c7f386a9ec5b6df9ebb54ccdf1b5/packages/fxa-auth-server/lib/oauth/routes/token.js#L70-L152

#[derive(Serialize)]
#[serde(tag = "grant_type")]
enum OAauthTokenRequest {
    #[serde(rename = "refresh_token")]
    UsingRefreshToken {
        client_id: String,
        refresh_token: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ttl: Option<u64>,
    },
    #[serde(rename = "authorization_code")]
    UsingCode {
        client_id: String,
        code: String,
        code_verifier: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        ttl: Option<u64>,
    },
}

#[derive(Deserialize)]
pub struct OAuthTokenResponse {
    pub keys_jwe: Option<String>,
    pub refresh_token: Option<String>,
    pub session_token: Option<String>,
    pub expires_in: u64,
    pub scope: String,
    pub access_token: String,
}

#[derive(Deserialize, Debug)]
pub struct OAuthAuthResponse {
    pub redirect: String,
    pub code: String,
    pub state: String,
}

#[derive(Deserialize)]
pub struct IntrospectResponse {
    pub active: bool,
    // Technically the response has a lot of other fields,
    // but in practice we only use `active`.
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileResponse {
    pub uid: String,
    pub email: String,
    pub display_name: Option<String>,
    pub avatar: String,
    pub avatar_default: bool,
}

impl From<ProfileResponse> for crate::Profile {
    fn from(p: ProfileResponse) -> Self {
        crate::Profile {
            uid: p.uid,
            email: p.email,
            display_name: p.display_name,
            avatar: p.avatar,
            is_default_avatar: p.avatar_default,
        }
    }
}

#[derive(Deserialize)]
pub struct ScopedKeyDataResponse {
    pub identifier: String,
    #[serde(rename = "keyRotationSecret")]
    pub key_rotation_secret: String,
    #[serde(rename = "keyRotationTimestamp")]
    pub key_rotation_timestamp: u64,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DuplicateTokenResponse {
    pub uid: String,
    #[serde(rename = "sessionToken")]
    pub session_token: String,
    pub verified: bool,
    #[serde(rename = "authAt")]
    pub auth_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::mock;
    #[test]
    #[allow(non_snake_case)]
    fn check_OAauthTokenRequest_serialization() {
        // Ensure OAauthTokenRequest serializes to what the server expects.
        let using_code = OAauthTokenRequest::UsingCode {
            code: "foo".to_owned(),
            client_id: "bar".to_owned(),
            code_verifier: "bobo".to_owned(),
            ttl: None,
        };
        assert_eq!("{\"grant_type\":\"authorization_code\",\"client_id\":\"bar\",\"code\":\"foo\",\"code_verifier\":\"bobo\"}", serde_json::to_string(&using_code).unwrap());
        let using_code = OAauthTokenRequest::UsingRefreshToken {
            client_id: "bar".to_owned(),
            refresh_token: "foo".to_owned(),
            scope: Some("bobo".to_owned()),
            ttl: Some(123),
        };
        assert_eq!("{\"grant_type\":\"refresh_token\",\"client_id\":\"bar\",\"refresh_token\":\"foo\",\"scope\":\"bobo\",\"ttl\":123}", serde_json::to_string(&using_code).unwrap());
    }

    #[test]
    fn test_backoff() {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock("POST", "/v1/account/devices/invoke_command")
            .with_status(429)
            .with_header("Content-Type", "application/json")
            .with_header("retry-after", "1000000")
            .with_body(
                r#"{
                "code": 429,
                "errno": 120,
                "error": "Too many requests",
                "message": "Too many requests",
                "retryAfter": 1000000,
                "info": "Some information"
            }"#,
            )
            .create();
        let client = Client::new();
        let path = format!(
            "{}/{}",
            mockito::server_url(),
            "v1/account/devices/invoke_command"
        );
        let url = Url::parse(&path).unwrap();
        let path = url.path().to_string();
        let request = Request::post(url);
        assert!(client.make_request(request.clone()).is_err());
        let state = client.state.lock();
        if let HttpClientState::Backoff {
            backoff_end_duration,
            time_since_backoff: _,
        } = state.get(&path).unwrap()
        {
            assert_eq!(*backoff_end_duration, Duration::from_secs(1_000_000));
            // Hacky way to drop the mutex gaurd, so that the next call to
            // client.make_request doesn't hang or panic
            std::mem::drop(state);
            assert!(client.make_request(request).is_err());
            // We should be backed off, the second "make_request" should not
            // send a request to the server
            m.expect(1).assert();
        } else {
            panic!("HttpClientState should be a timeout!");
        }
    }

    #[test]
    fn test_backoff_then_ok() {
        viaduct_reqwest::use_reqwest_backend();
        let m = mock("POST", "/v1/account/devices/invoke_command")
            .with_status(429)
            .with_header("Content-Type", "application/json")
            .with_header("retry-after", "1")
            .with_body(
                r#"{
                "code": 429,
                "errno": 120,
                "error": "Too many requests",
                "message": "Too many requests",
                "retryAfter": 1,
                "info": "Some information"
            }"#,
            )
            .create();
        let client = Client::new();
        let path = format!(
            "{}/{}",
            mockito::server_url(),
            "v1/account/devices/invoke_command"
        );
        let url = Url::parse(&path).unwrap();
        let path = url.path().to_string();
        let request = Request::post(url);
        assert!(client.make_request(request.clone()).is_err());
        let state = client.state.lock();
        if let HttpClientState::Backoff {
            backoff_end_duration,
            time_since_backoff: _,
        } = state.get(&path).unwrap()
        {
            assert_eq!(*backoff_end_duration, Duration::from_secs(1));
            // We sleep for 1 second, so pass the backoff timeout
            std::thread::sleep(*backoff_end_duration);

            // Hacky way to drop the mutex gaurd, so that the next call to
            // client.make_request doesn't hang or panic
            std::mem::drop(state);
            assert!(client.make_request(request).is_err());
            // We backed off, but the time has passed, the second request should have
            // went to the server
            m.expect(2).assert();
        } else {
            panic!("HttpClientState should be a timeout!");
        }
    }

    #[test]
    fn test_backoff_per_path() {
        viaduct_reqwest::use_reqwest_backend();
        let m1 = mock("POST", "/v1/account/devices/invoke_command")
            .with_status(429)
            .with_header("Content-Type", "application/json")
            .with_header("retry-after", "1000000")
            .with_body(
                r#"{
                "code": 429,
                "errno": 120,
                "error": "Too many requests",
                "message": "Too many requests",
                "retryAfter": 1000000,
                "info": "Some information"
            }"#,
            )
            .create();
        let m2 = mock("GET", "/v1/account/device/commands")
            .with_status(200)
            .with_header("Content-Type", "application/json")
            .with_body(
                r#"
        {
         "index": 3,
         "last": true,
         "messages": []
        }"#,
            )
            .create();
        let client = Client::new();
        let path = format!(
            "{}/{}",
            mockito::server_url(),
            "v1/account/devices/invoke_command"
        );
        let url = Url::parse(&path).unwrap();
        let path = url.path().to_string();
        let request = Request::post(url);
        assert!(client.make_request(request).is_err());
        let state = client.state.lock();
        if let HttpClientState::Backoff {
            backoff_end_duration,
            time_since_backoff: _,
        } = state.get(&path).unwrap()
        {
            assert_eq!(*backoff_end_duration, Duration::from_secs(1_000_000));

            let path2 = format!("{}/{}", mockito::server_url(), "v1/account/device/commands");
            // Hacky way to drop the mutex guard, so that the next call to
            // client.make_request doesn't hang or panic
            std::mem::drop(state);
            let second_request = Request::get(Url::parse(&path2).unwrap());
            assert!(client.make_request(second_request).is_ok());
            // The first endpoint is backed off, but the second one is not
            // Both endpoint should be hit
            m1.expect(1).assert();
            m2.expect(1).assert();
        } else {
            panic!("HttpClientState should be a timeout!");
        }
    }
}

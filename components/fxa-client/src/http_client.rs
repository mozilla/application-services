/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{config::Config, errors::*};
use browser_id::{derive_key_from_session_token, hawk_request::HawkRequestBuilder};
use serde_derive::*;
use serde_json::json;
use viaduct::{header_names, status_codes, Request, Response, Method};
use std::collections::HashMap;

pub(crate) mod browser_id;

pub trait FxAClient {
    fn oauth_token_with_code(
        &self,
        config: &Config,
        code: &str,
        code_verifier: &str,
    ) -> Result<OAuthTokenResponse>;
    fn oauth_token_with_refresh_token(
        &self,
        config: &Config,
        refresh_token: &str,
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse>;
    fn refresh_token_with_session_token(
        &self,
        config: &Config,
        session_token: &[u8],
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse>;
    fn destroy_oauth_token(&self, config: &Config, token: &str) -> Result<()>;
    fn profile(
        &self,
        config: &Config,
        profile_access_token: &str,
        etag: Option<String>,
    ) -> Result<Option<ResponseAndETag<ProfileResponse>>>;
    fn scoped_key_data(
        &self,
        config: &Config,
        session_token: &[u8],
        scope: &str,
    ) -> Result<HashMap<String, ScopedKeyDataResponse>>;
}

pub struct Client;
impl FxAClient for Client {
    fn profile(
        &self,
        config: &Config,
        profile_access_token: &str,
        etag: Option<String>,
    ) -> Result<Option<ResponseAndETag<ProfileResponse>>> {
        let url = config.userinfo_endpoint()?;
        let mut request = Request::get(url).header(
            header_names::AUTHORIZATION,
            format!("Bearer {}", profile_access_token),
        )?;
        if let Some(etag) = etag {
            request = request.header(header_names::IF_NONE_MATCH, format!("\"{}\"", etag))?;
        }
        let resp = Self::make_request(request)?;
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

    fn oauth_token_with_code(
        &self,
        config: &Config,
        code: &str,
        code_verifier: &str,
    ) -> Result<OAuthTokenResponse> {
        let body = json!({
            "code": code,
            "client_id": config.client_id,
            "code_verifier": code_verifier
        });
        self.make_oauth_token_request(config, body)
    }

    fn oauth_token_with_refresh_token(
        &self,
        config: &Config,
        refresh_token: &str,
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse> {
        let body = json!({
            "grant_type": "refresh_token",
            "client_id": config.client_id,
            "refresh_token": refresh_token,
            "scope": scopes.join(" ")
        });
        self.make_oauth_token_request(config, body)
    }

    fn refresh_token_with_session_token(
        &self,
        config: &Config,
        session_token: &[u8],
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse> {
        let url = config.auth_url_path("v1/oauth/authorization")?;
        let key = derive_key_from_session_token(session_token)?;
        let body = json!({
            "client_id": config.client_id,
            "scope": scopes.join(" "),
            "response_type": "token",
            "access_type": "offline",
        });
        let request = HawkRequestBuilder::new(Method::Post, url, &key)
            .body(body)
            .build()?;
        Ok(Self::make_request(request)?.json()?)
    }

    fn destroy_oauth_token(&self, config: &Config, token: &str) -> Result<()> {
        let body = json!({
            "token": token,
        });
        let url = config.oauth_url_path("v1/destroy")?;
        Self::make_request(Request::post(url).json(&body))?;
        Ok(())
    }

    fn scoped_key_data(
        &self,
        config: &Config,
        session_token: &[u8],
        scope: &str,
    ) -> Result<HashMap<String, ScopedKeyDataResponse>> {
        let body = json!({
            "client_id": config.client_id,
            "scope": scope,
        });
        let url = config.auth_url_path("v1/account/scoped-key-data")?;
        let key = derive_key_from_session_token(session_token)?;
        let request = HawkRequestBuilder::new(Method::Post, url, &key)
            .body(body)
            .build()?;
        Self::make_request(request)?.json().map_err(|e| e.into())
    }
}

impl Client {
    pub fn new() -> Self {
        Self {}
    }
    fn make_oauth_token_request(
        &self,
        config: &Config,
        body: serde_json::Value,
    ) -> Result<OAuthTokenResponse> {
        let url = config.token_endpoint()?;
        Self::make_request(Request::post(url).json(&body))?
            .json()
            .map_err(Into::into)
    }

    fn make_request(request: Request) -> Result<Response> {
        let resp = request.send()?;
        if resp.is_success() || resp.status == status_codes::NOT_MODIFIED {
            Ok(resp)
        } else {
            let json: std::result::Result<serde_json::Value, _> = resp.json();
            match json {
                Ok(json) => Err(ErrorKind::RemoteError {
                    code: json["code"].as_u64().unwrap_or(0),
                    errno: json["errno"].as_u64().unwrap_or(0),
                    error: json["error"].as_str().unwrap_or("").to_string(),
                    message: json["message"].as_str().unwrap_or("").to_string(),
                    info: json["info"].as_str().unwrap_or("").to_string(),
                }
                .into()),
                Err(_) => Err(resp.require_success().unwrap_err().into()),
            }
        }
    }
}

#[derive(Clone)]
pub struct ResponseAndETag<T> {
    pub response: T,
    pub etag: Option<String>,
}

#[derive(Deserialize)]
pub struct OAuthTokenResponse {
    pub keys_jwe: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    #[serde(default)] // TODO: workaround OAuth server bug.
    pub scope: String,
    pub access_token: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProfileResponse {
    pub uid: String,
    pub email: String,
    pub locale: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub avatar: String,
    #[serde(rename = "avatarDefault")]
    pub avatar_default: bool,
    #[serde(rename = "amrValues")]
    pub amr_values: Vec<String>,
    #[serde(rename = "twoFactorAuthentication")]
    pub two_factor_authentication: bool,
}

#[derive(Deserialize)]
pub struct ScopedKeyDataResponse {
    pub identifier: String,
    #[serde(rename = "keyRotationSecret")]
    pub key_rotation_secret: String,
    #[serde(rename = "keyRotationTimestamp")]
    pub key_rotation_timestamp: u64,
}

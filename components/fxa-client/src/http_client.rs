/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{config::Config, errors::*};
use reqwest::{self, header, Client as ReqwestClient, Method, Request, Response, StatusCode};
use serde_derive::*;
use serde_json::json;

#[cfg(feature = "browserid")]
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
    fn destroy_oauth_token(&self, config: &Config, token: &str) -> Result<()>;
    fn profile(
        &self,
        config: &Config,
        profile_access_token: &str,
        etag: Option<String>,
    ) -> Result<Option<ResponseAndETag<ProfileResponse>>>;
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
        let client = ReqwestClient::new();
        let mut builder = client.request(Method::GET, url).header(
            header::AUTHORIZATION,
            format!("Bearer {}", profile_access_token),
        );
        if let Some(etag) = etag {
            builder = builder.header(header::IF_NONE_MATCH, format!("\"{}\"", etag));
        }
        let request = builder.build()?;
        let mut resp = Self::make_request(request)?;
        if resp.status() == StatusCode::NOT_MODIFIED {
            return Ok(None);
        }
        let etag = resp
            .headers()
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_owned());
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

    fn destroy_oauth_token(&self, config: &Config, token: &str) -> Result<()> {
        let body = json!({
            "token": token,
        });
        let url = config.oauth_url_path("v1/destroy")?;
        let client = ReqwestClient::new();
        let request = client
            .request(Method::POST, url)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .build()?;
        Self::make_request(request)?;
        Ok(())
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
        let client = ReqwestClient::new();
        let request = client
            .request(Method::POST, url)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .build()?;
        Self::make_request(request)?.json().map_err(|e| e.into())
    }

    fn make_request(request: Request) -> Result<Response> {
        let client = ReqwestClient::new();
        let mut resp = client.execute(request)?;
        let status = resp.status();

        if status.is_success() || status == StatusCode::NOT_MODIFIED {
            Ok(resp)
        } else {
            let json: std::result::Result<serde_json::Value, reqwest::Error> = resp.json();
            match json {
                Ok(json) => Err(ErrorKind::RemoteError {
                    code: json["code"].as_u64().unwrap_or(0),
                    errno: json["errno"].as_u64().unwrap_or(0),
                    error: json["error"].as_str().unwrap_or("").to_string(),
                    message: json["message"].as_str().unwrap_or("").to_string(),
                    info: json["info"].as_str().unwrap_or("").to_string(),
                }
                .into()),
                Err(_) => Err(resp.error_for_status().unwrap_err().into()),
            }
        }
    }
}

pub struct ResponseAndETag<T> {
    pub response: T,
    pub etag: Option<String>,
}

#[derive(Deserialize)]
pub struct OAuthTokenResponse {
    pub keys_jwe: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
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

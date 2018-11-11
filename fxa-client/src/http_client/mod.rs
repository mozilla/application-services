/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[cfg(feature = "browserid")]
use hex;
use reqwest;
use reqwest::{header, Client as ReqwestClient, Method, Request, Response, StatusCode};
#[cfg(feature = "browserid")]
use ring::{digest, hkdf, hmac};
use serde_json;
use std;
use std::collections::HashMap;
#[cfg(feature = "browserid")]
use util::Xorable;

#[cfg(feature = "browserid")]
use self::browser_id::rsa::RSABrowserIDKeyPair;
#[cfg(feature = "browserid")]
use self::browser_id::{jwt_utils, BrowserIDKeyPair};
#[cfg(feature = "browserid")]
use self::hawk_request::HAWKRequestBuilder;
use config::Config;
use errors::*;

#[cfg(feature = "browserid")]
pub mod browser_id;
#[cfg(feature = "browserid")]
mod hawk_request;

#[cfg(feature = "browserid")]
const HKDF_SALT: [u8; 32] = [0b0; 32];
#[cfg(feature = "browserid")]
const KEY_LENGTH: usize = 32;
#[cfg(feature = "browserid")]
const SIGN_DURATION_MS: u64 = 24 * 60 * 60 * 1000;

pub struct Client<'a> {
    config: &'a Config,
}

impl<'a> Client<'a> {
    pub fn new(config: &'a Config) -> Client<'a> {
        Client { config }
    }

    #[cfg(feature = "browserid")]
    fn kw(name: &str) -> Vec<u8> {
        format!("identity.mozilla.com/picl/v1/{}", name)
            .as_bytes()
            .to_vec()
    }

    #[allow(dead_code)]
    fn kwe(name: &str, email: &str) -> Vec<u8> {
        format!("identity.mozilla.com/picl/v1/{}:{}", name, email)
            .as_bytes()
            .to_vec()
    }

    #[cfg(feature = "browserid")]
    pub fn key_pair(len: u32) -> Result<RSABrowserIDKeyPair> {
        RSABrowserIDKeyPair::generate_random(len)
    }

    #[cfg(feature = "browserid")]
    pub fn derive_sync_key(kb: &[u8]) -> Vec<u8> {
        let salt = [0u8; 0];
        let context_info = Self::kw("oldsync");
        Self::derive_hkdf_sha256_key(&kb, &salt, &context_info, KEY_LENGTH * 2)
    }

    #[cfg(feature = "browserid")]
    pub fn compute_client_state(kb: &[u8]) -> String {
        hex::encode(digest::digest(&digest::SHA256, &kb).as_ref()[0..16].to_vec())
    }

    #[cfg(feature = "browserid")]
    pub fn sign_out(&self) {
        panic!("Not implemented yet!");
    }

    #[cfg(feature = "browserid")]
    pub fn login(&self, email: &str, auth_pwd: &str, get_keys: bool) -> Result<LoginResponse> {
        let url = self.config.auth_url_path("v1/account/login")?;
        let parameters = json!({
            "email": email,
            "authPW": auth_pwd
        });
        let client = ReqwestClient::new();
        let request = client
            .request(Method::POST, url)
            .query(&[("keys", get_keys)])
            .body(parameters.to_string())
            .build()?;
        Self::make_request(request)?.json().map_err(|e| e.into())
    }

    #[cfg(feature = "browserid")]
    pub fn account_status(&self, uid: &String) -> Result<AccountStatusResponse> {
        let url = self.config.auth_url_path("v1/account/status")?;
        let client = ReqwestClient::new();
        let request = client.get(url).query(&[("uid", uid)]).build()?;
        Self::make_request(request)?.json().map_err(|e| e.into())
    }

    #[cfg(feature = "browserid")]
    pub fn keys(&self, key_fetch_token: &[u8]) -> Result<KeysResponse> {
        let url = self.config.auth_url_path("v1/account/keys")?;
        let context_info = Self::kw("keyFetchToken");
        let key = Self::derive_hkdf_sha256_key(
            &key_fetch_token,
            &HKDF_SALT,
            &context_info,
            KEY_LENGTH * 3,
        );
        let key_request_key = &key[(KEY_LENGTH * 2)..(KEY_LENGTH * 3)];
        let request = HAWKRequestBuilder::new(Method::GET, url, &key).build()?;
        let json: serde_json::Value = Self::make_request(request)?.json()?;
        let bundle = match json["bundle"].as_str() {
            Some(bundle) => bundle,
            None => panic!("Invalid JSON"),
        };
        let data = hex::decode(bundle)?;
        if data.len() != 3 * KEY_LENGTH {
            return Err(ErrorKind::BadKeyLength("bundle", 3 * KEY_LENGTH, data.len()).into());
        }
        let ciphertext = &data[0..(KEY_LENGTH * 2)];
        let mac_code = &data[(KEY_LENGTH * 2)..(KEY_LENGTH * 3)];
        let context_info = Self::kw("account/keys");
        let bytes = Self::derive_hkdf_sha256_key(
            key_request_key,
            &HKDF_SALT,
            &context_info,
            KEY_LENGTH * 3,
        );
        let hmac_key = &bytes[0..KEY_LENGTH];
        let xor_key = &bytes[KEY_LENGTH..(KEY_LENGTH * 3)];

        let v_key = hmac::VerificationKey::new(&digest::SHA256, hmac_key.as_ref());
        hmac::verify(&v_key, ciphertext, mac_code).map_err(|_| ErrorKind::HmacVerifyFail)?;

        let xored_bytes = ciphertext.xored_with(xor_key)?;
        let wrap_kb = xored_bytes[KEY_LENGTH..(KEY_LENGTH * 2)].to_vec();
        Ok(KeysResponse { wrap_kb })
    }

    #[cfg(feature = "browserid")]
    pub fn recovery_email_status(
        &self,
        session_token: &[u8],
    ) -> Result<RecoveryEmailStatusResponse> {
        let url = self.config.auth_url_path("v1/recovery_email/status")?;
        let key = Self::derive_key_from_session_token(session_token)?;
        let request = HAWKRequestBuilder::new(Method::GET, url, &key).build()?;
        Self::make_request(request)?.json().map_err(|e| e.into())
    }

    pub fn profile(
        &self,
        access_token: &str,
        etag: Option<String>,
    ) -> Result<Option<ResponseAndETag<ProfileResponse>>> {
        let url = self.config.userinfo_endpoint()?;
        let client = ReqwestClient::new();
        let mut builder = client
            .request(Method::GET, url)
            .header(header::AUTHORIZATION, Self::bearer_token(access_token));
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

    #[cfg(feature = "browserid")]
    pub fn oauth_token_with_session_token(
        &self,
        session_token: &[u8],
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse> {
        let audience = self.get_oauth_audience()?;
        let key_pair = Self::key_pair(1024)?;
        let certificate = self.sign(session_token, &key_pair)?.certificate;
        let assertion = jwt_utils::create_assertion(&key_pair, &certificate, &audience)?;
        let parameters = json!({
            "assertion": assertion,
            "client_id": self.config.client_id,
            "response_type": "token",
            "scope": scopes.join(" ")
        });
        let key = Self::derive_key_from_session_token(session_token)?;
        let url = self.config.authorization_endpoint()?;
        let request = HAWKRequestBuilder::new(Method::POST, url, &key)
            .body(parameters)
            .build()?;
        Self::make_request(request)?.json().map_err(|e| e.into())
    }

    pub fn oauth_token_with_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<OAuthTokenResponse> {
        let body = json!({
            "code": code,
            "client_id": self.config.client_id,
            "code_verifier": code_verifier
        });
        self.make_oauth_token_request(body)
    }

    pub fn oauth_token_with_refresh_token(
        &self,
        refresh_token: &str,
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse> {
        let body = json!({
            "grant_type": "refresh_token",
            "client_id": self.config.client_id,
            "refresh_token": refresh_token,
            "scope": scopes.join(" ")
        });
        self.make_oauth_token_request(body)
    }

    fn make_oauth_token_request(&self, body: serde_json::Value) -> Result<OAuthTokenResponse> {
        let url = self.config.token_endpoint()?;
        let client = ReqwestClient::new();
        let request = client
            .request(Method::POST, url)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .build()?;
        Self::make_request(request)?.json().map_err(|e| e.into())
    }

    pub fn destroy_oauth_token(&self, token: &str) -> Result<()> {
        let body = json!({
            "token": token,
        });
        let url = self.config.oauth_url_path("v1/destroy")?;
        let client = ReqwestClient::new();
        let request = client
            .request(Method::POST, url)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .build()?;
        Self::make_request(request)?;
        Ok(())
    }

    pub fn pending_commands(
        &self,
        refresh_token: &str,
        index: i64,
        limit: Option<i64>,
    ) -> Result<PendingCommandsResponse> {
        let url = self
            .config
            .auth_url_path("v1/client_instance/pending_commands")?;
        let client = ReqwestClient::new();
        let mut builder = client
            .request(Method::GET, url)
            .header(header::AUTHORIZATION, Self::bearer_token(refresh_token))
            .query(&[("index", index)]);
        if let Some(limit) = limit {
            builder = builder.query(&[("limit", limit)])
        }
        let request = builder.build()?;
        Self::make_request(request)?.json().map_err(|e| e.into())
    }

    pub fn invoke_command(
        &self,
        access_token: &str,
        command: &str,
        target: &str,
        payload: &serde_json::Value,
    ) -> Result<()> {
        let body = json!({
            "command": command,
            "target": target,
            "payload": payload
        });
        let url = self
            .config
            .auth_url_path("v1/clients_instances/invoke_command")?;
        let client = ReqwestClient::new();
        let request = client
            .request(Method::POST, url)
            .header(header::AUTHORIZATION, Self::bearer_token(access_token))
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .build()?;
        Self::make_request(request)?;
        Ok(())
    }

    pub fn instance(&self, refresh_token: &str) -> Result<ClientInstanceResponse> {
        let url = self.config.auth_url_path("v1/client_instance")?;
        let client = ReqwestClient::new();
        let request = client
            .request(Method::GET, url)
            .header(header::AUTHORIZATION, Self::bearer_token(refresh_token))
            .build()?;
        Self::make_request(request)?.json().map_err(|e| e.into())
    }

    pub fn instances(&self, access_token: &str) -> Result<Vec<ClientInstanceResponse>> {
        let url = self.config.auth_url_path("v1/clients_instances")?;
        let client = ReqwestClient::new();
        let request = client
            .request(Method::GET, url)
            .header(header::AUTHORIZATION, Self::bearer_token(access_token))
            .build()?;
        Self::make_request(request)?.json().map_err(|e| e.into())
    }

    pub fn upsert_instance(
        &self,
        refresh_token: &str,
        metadata: ClientInstanceRequest,
    ) -> Result<()> {
        let body = serde_json::to_string(&metadata)?;
        let url = self.config.auth_url_path("v1/client_instance")?;
        let client = ReqwestClient::new();
        let request = client
            .request(Method::POST, url)
            .header(header::AUTHORIZATION, Self::bearer_token(refresh_token))
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .build()?;
        Self::make_request(request)?;
        Ok(())
    }

    pub fn patch_instance_commands(
        &self,
        refresh_token: &str,
        commands: HashMap<String, Option<String>>,
    ) -> Result<()> {
        let body = serde_json::to_string(&commands)?;
        let url = self.config.auth_url_path("v1/client_instance/commands")?;
        let client = ReqwestClient::new();
        let request = client
            .request(Method::PATCH, url)
            .header(header::AUTHORIZATION, Self::bearer_token(refresh_token))
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .build()?;
        Self::make_request(request)?;
        Ok(())
    }

    #[cfg(feature = "browserid")]
    pub fn sign(&self, session_token: &[u8], key_pair: &BrowserIDKeyPair) -> Result<SignResponse> {
        let public_key_json = key_pair.to_json(false)?;
        let parameters = json!({
            "publicKey": public_key_json,
            "duration": SIGN_DURATION_MS
        });
        let key = Self::derive_key_from_session_token(session_token)?;
        let url = self.config.auth_url_path("v1/certificate/sign")?;
        let request = HAWKRequestBuilder::new(Method::POST, url, &key)
            .body(parameters)
            .build()?;
        Self::make_request(request)?.json().map_err(|e| e.into())
    }

    #[cfg(feature = "browserid")]
    fn get_oauth_audience(&self) -> Result<String> {
        let url = self.config.oauth_url()?;
        let host = url
            .host_str()
            .ok_or_else(|| ErrorKind::AudienceURLWithoutHost)?;
        match url.port() {
            Some(port) => Ok(format!("{}://{}:{}", url.scheme(), host, port)),
            None => Ok(format!("{}://{}", url.scheme(), host)),
        }
    }

    #[cfg(feature = "browserid")]
    fn derive_key_from_session_token(session_token: &[u8]) -> Result<Vec<u8>> {
        let context_info = Self::kw("sessionToken");
        Ok(Client::derive_hkdf_sha256_key(
            session_token,
            &HKDF_SALT,
            &context_info,
            KEY_LENGTH * 2,
        ))
    }

    #[cfg(feature = "browserid")]
    fn derive_hkdf_sha256_key(ikm: &[u8], salt: &[u8], info: &[u8], len: usize) -> Vec<u8> {
        let salt = hmac::SigningKey::new(&digest::SHA256, salt);
        let mut out = vec![0u8; len];
        hkdf::extract_and_expand(&salt, ikm, info, &mut out);
        out.to_vec()
    }

    fn bearer_token(token: &str) -> String {
        format!("Bearer {}", token)
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

#[derive(Deserialize)]
pub struct CommandData {
    pub command: String, // In the future try to make it an enum.
    pub payload: serde_json::Value,
    pub sender: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct PushSubscription {
    #[serde(rename = "pushEndpoint")]
    pub endpoint: String,
    #[serde(rename = "pushPublicKey")]
    pub public_key: String,
    #[serde(rename = "pushAuthKey")]
    pub auth_key: String,
}

/// We use the double Option pattern in this struct.
/// The outer option represents the existence of the field
/// and the inner option its value or null.
/// TL;DR:
/// `None`: the field will not be present in the JSON body.
/// `Some(None)`: the field will have a `null` value.
/// `Some(Some(T))`: the field will have the serialized value of T.
#[derive(Serialize)]
pub struct ClientInstanceRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<Option<String>>,
    #[serde(flatten)]
    push_subscription: Option<PushSubscription>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "availableCommands")]
    available_commands: Option<Option<HashMap<String, String>>>,
}

pub struct ClientInstanceRequestBuilder {
    name: Option<Option<String>>,
    push_subscription: Option<PushSubscription>,
    available_commands: Option<Option<HashMap<String, String>>>,
}

impl ClientInstanceRequestBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            push_subscription: None,
            available_commands: None,
        }
    }

    pub fn push_subscription(
        mut self,
        push_subscription: PushSubscription,
    ) -> ClientInstanceRequestBuilder {
        self.push_subscription = Some(push_subscription);
        self
    }

    pub fn clear_available_commands(mut self) -> ClientInstanceRequestBuilder {
        self.available_commands = Some(None);
        self
    }

    pub fn name(mut self, name: &str) -> ClientInstanceRequestBuilder {
        self.name = Some(Some(name.to_string()));
        self
    }

    pub fn clear_name(mut self) -> ClientInstanceRequestBuilder {
        self.name = Some(None);
        self
    }

    pub fn build(self) -> ClientInstanceRequest {
        ClientInstanceRequest {
            name: self.name,
            push_subscription: self.push_subscription,
            available_commands: self.available_commands,
        }
    }
}

#[derive(Deserialize)]
pub struct ClientInstanceResponse {
    pub id: String,
    #[serde(rename = "clientId")]
    pub client_id: String,
    pub name: Option<String>,
    #[serde(flatten)]
    pub push_subscription: Option<PushSubscription>,
    #[serde(rename = "availableCommands")]
    pub available_commands: HashMap<String, String>,
}

#[derive(Deserialize)]
pub struct LoginResponse {
    pub uid: String,
    #[serde(rename = "sessionToken")]
    pub session_token: String,
    pub verified: bool,
}

#[derive(Deserialize)]
pub struct RecoveryEmailStatusResponse {
    pub email: String,
    pub verified: bool,
}

#[derive(Deserialize)]
pub struct AccountStatusResponse {
    pub exists: bool,
}

#[derive(Deserialize)]
pub struct OAuthTokenResponse {
    pub keys_jwe: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    pub scope: String,
    pub access_token: String,
}

#[derive(Deserialize)]
pub struct SignResponse {
    #[serde(rename = "cert")]
    pub certificate: String,
}

#[derive(Deserialize)]
pub struct KeysResponse {
    pub wrap_kb: Vec<u8>,
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

#[cfg(test)]
#[cfg(feature = "browserid")]
mod tests {
    use super::*;
    use ring::{digest, pbkdf2};

    fn quick_strech_pwd(email: &str, pwd: &str) -> Vec<u8> {
        let salt = Self::kwe("quickStretch", email);
        let mut out = [0u8; 32];
        pbkdf2::derive(&digest::SHA256, 1000, &salt, pwd.as_bytes(), &mut out);
        out.to_vec()
    }

    fn auth_pwd(email: &str, pwd: &str) -> String {
        let streched = quick_strech_pwd(email, pwd);
        let salt = [0u8; 0];
        let context = Self::kw("authPW");
        let derived = Self::derive_hkdf_sha256_key(&streched, &salt, &context, 32);
        hex::encode(derived)
    }

    #[test]
    fn test_quick_strech_pwd() {
        let email = "andré@example.org";
        let pwd = "pässwörd";
        let streched = hex::encode(quick_strech_pwd(email, pwd));
        assert_eq!(
            streched,
            "e4e8889bd8bd61ad6de6b95c059d56e7b50dacdaf62bd84644af7e2add84345d"
        );
    }

    #[test]
    fn test_auth_pwd() {
        let email = "andré@example.org";
        let pwd = "pässwörd";
        let auth_pwd = auth_pwd(email, pwd);
        assert_eq!(
            auth_pwd,
            "247b675ffb4c46310bc87e26d712153abe5e1c90ef00a4784594f97ef54f2375"
        );
    }
}

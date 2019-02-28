/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::errors::*;
#[cfg(feature = "browserid")]
use crate::{
    http_client::{self, OAuthTokenResponse},
    util::Xorable,
    Config,
};
use hawk_request::HawkRequestBuilder;
use reqwest::{Client as ReqwestClient, Method, Url};
use ring::{digest, hkdf, hmac};
#[cfg(feature = "browserid")]
use rsa::RSABrowserIDKeyPair;
#[cfg(feature = "browserid")]
use serde_derive::*;
#[cfg(feature = "browserid")]
use serde_json::json;
use url::Url;
use viaduct::{Method, Request};
pub(crate) mod hawk_request;
#[cfg(feature = "browserid")]
pub(crate) mod jwt_utils;
#[cfg(feature = "browserid")]
pub(crate) mod rsa;

const HKDF_SALT: [u8; 32] = [0b0; 32];
const KEY_LENGTH: usize = 32;
#[cfg(feature = "browserid")]
const SIGN_DURATION_MS: u64 = 24 * 60 * 60 * 1000;

#[cfg(feature = "browserid")]
pub trait BrowserIDKeyPair {
    fn get_algo(&self) -> String;
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>>;
    fn verify_message(&self, message: &[u8], signature: &[u8]) -> Result<bool>;
    fn to_json(&self, include_private: bool) -> Result<serde_json::Value>;
}

#[cfg(feature = "browserid")]
pub trait FxABrowserIDClient: http_client::FxAClient {
    fn sign_out(&self);
    fn login(
        &self,
        config: &Config,
        email: &str,
        auth_pwd: &str,
        get_keys: bool,
    ) -> Result<LoginResponse>;
    fn account_status(&self, config: &Config, uid: &str) -> Result<AccountStatusResponse>;
    fn keys(&self, config: &Config, key_fetch_token: &[u8]) -> Result<KeysResponse>;
    fn recovery_email_status(
        &self,
        config: &Config,
        session_token: &[u8],
    ) -> Result<RecoveryEmailStatusResponse>;
    fn oauth_token_with_session_token(
        &self,
        config: &Config,
        session_token: &[u8],
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse>;
    fn sign(
        &self,
        config: &Config,
        session_token: &[u8],
        key_pair: &dyn BrowserIDKeyPair,
    ) -> Result<SignResponse>;
}

#[cfg(feature = "browserid")]
impl FxABrowserIDClient for http_client::Client {
    fn sign_out(&self) {
        panic!("Not implemented yet!");
    }

    fn login(
        &self,
        config: &Config,
        email: &str,
        auth_pwd: &str,
        get_keys: bool,
    ) -> Result<LoginResponse> {
        let url = config.auth_url_path("v1/account/login")?;
        let parameters = json!({
            "email": email,
            "authPW": auth_pwd
        });
        let request = Request::post(url)
            .query(&[("keys", if get_keys { "true" } else { "false" })])
            .json(&parameters);
        Self::make_request(request)?.json().map_err(Into::into)
    }

    fn account_status(&self, config: &Config, uid: &str) -> Result<AccountStatusResponse> {
        let url = config.auth_url_path("v1/account/status")?;
        let request = Request::get(url).query(&[("uid", uid)]);
        Self::make_request(request)?.json().map_err(Into::into)
    }

    fn keys(&self, config: &Config, key_fetch_token: &[u8]) -> Result<KeysResponse> {
        let url = config.auth_url_path("v1/account/keys")?;
        let context_info = kw("keyFetchToken");
        let key =
            derive_hkdf_sha256_key(&key_fetch_token, &HKDF_SALT, &context_info, KEY_LENGTH * 3);
        let key_request_key = &key[(KEY_LENGTH * 2)..(KEY_LENGTH * 3)];
        let request = HawkRequestBuilder::new(Method::Get, url, &key).build()?;
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
        let context_info = kw("account/keys");
        let bytes =
            derive_hkdf_sha256_key(key_request_key, &HKDF_SALT, &context_info, KEY_LENGTH * 3);
        let hmac_key = &bytes[0..KEY_LENGTH];
        let xor_key = &bytes[KEY_LENGTH..(KEY_LENGTH * 3)];

        let v_key = hmac::VerificationKey::new(&digest::SHA256, hmac_key);
        hmac::verify(&v_key, ciphertext, mac_code).map_err(|_| ErrorKind::HmacVerifyFail)?;

        let xored_bytes = ciphertext.xored_with(xor_key)?;
        let wrap_kb = xored_bytes[KEY_LENGTH..(KEY_LENGTH * 2)].to_vec();
        Ok(KeysResponse { wrap_kb })
    }

    fn recovery_email_status(
        &self,
        config: &Config,
        session_token: &[u8],
    ) -> Result<RecoveryEmailStatusResponse> {
        let url = config.auth_url_path("v1/recovery_email/status")?;
        let key = derive_key_from_session_token(session_token)?;
        let request = HawkRequestBuilder::new(Method::Get, url, &key).build()?;
        Self::make_request(request)?.json().map_err(Into::into)
    }

    fn oauth_token_with_session_token(
        &self,
        config: &Config,
        session_token: &[u8],
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse> {
        let audience = get_oauth_audience(&config.oauth_url()?)?;
        let key_pair = key_pair(1024)?;
        let certificate = self.sign(config, session_token, &key_pair)?.certificate;
        let assertion = jwt_utils::create_assertion(&key_pair, &certificate, &audience)?;
        let parameters = json!({
            "assertion": assertion,
            "client_id": config.client_id,
            "response_type": "token",
            "scope": scopes.join(" ")
        });
        let key = derive_key_from_session_token(session_token)?;
        let url = config.authorization_endpoint()?;
        let request = HawkRequestBuilder::new(Method::Post, url, &key)
            .body(parameters)
            .build()?;
        Self::make_request(request)?.json().map_err(Into::into)
    }

    fn sign(
        &self,
        config: &Config,
        session_token: &[u8],
        key_pair: &dyn BrowserIDKeyPair,
    ) -> Result<SignResponse> {
        let public_key_json = key_pair.to_json(false)?;
        let parameters = json!({
            "publicKey": public_key_json,
            "duration": SIGN_DURATION_MS
        });
        let key = derive_key_from_session_token(session_token)?;
        let url = config.auth_url_path("v1/certificate/sign")?;
        let request = HawkRequestBuilder::new(Method::Post, url, &key)
            .body(parameters)
            .build()?;
        Self::make_request(request)?.json().map_err(Into::into)
    }
}

pub(crate) fn derive_key_from_session_token(session_token: &[u8]) -> Result<Vec<u8>> {
    let context_info = kw("sessionToken");
    Ok(derive_hkdf_sha256_key(
        session_token,
        &HKDF_SALT,
        &context_info,
        KEY_LENGTH * 2,
    ))
}

fn kw(name: &str) -> Vec<u8> {
    format!("identity.mozilla.com/picl/v1/{}", name)
        .as_bytes()
        .to_vec()
}

fn derive_hkdf_sha256_key(ikm: &[u8], salt: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    let salt = hmac::SigningKey::new(&digest::SHA256, salt);
    let mut out = vec![0u8; len];
    hkdf::extract_and_expand(&salt, ikm, info, &mut out);
    out.to_vec()
}

#[cfg(feature = "browserid")]
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
pub(crate) fn derive_sync_key(kb: &[u8]) -> Vec<u8> {
    let salt = [0u8; 0];
    let context_info = kw("oldsync");
    derive_hkdf_sha256_key(&kb, &salt, &context_info, KEY_LENGTH * 2)
}

#[cfg(feature = "browserid")]
pub(crate) fn compute_client_state(kb: &[u8]) -> String {
    hex::encode(digest::digest(&digest::SHA256, &kb).as_ref()[0..16].to_vec())
}

#[cfg(feature = "browserid")]
fn get_oauth_audience(oauth_url: &Url) -> Result<String> {
    let host = oauth_url
        .host_str()
        .ok_or_else(|| ErrorKind::AudienceURLWithoutHost)?;
    match oauth_url.port() {
        Some(port) => Ok(format!("{}://{}:{}", oauth_url.scheme(), host, port)),
        None => Ok(format!("{}://{}", oauth_url.scheme(), host)),
    }
}

#[cfg(feature = "browserid")]
#[derive(Deserialize)]
pub struct LoginResponse {
    pub uid: String,
    #[serde(rename = "sessionToken")]
    pub session_token: String,
    pub verified: bool,
}

#[cfg(feature = "browserid")]
#[derive(Deserialize)]
pub struct RecoveryEmailStatusResponse {
    pub email: String,
    pub verified: bool,
}

#[cfg(feature = "browserid")]
#[derive(Deserialize)]
pub struct AccountStatusResponse {
    pub exists: bool,
}

#[cfg(feature = "browserid")]
#[derive(Deserialize)]
pub struct SignResponse {
    #[serde(rename = "cert")]
    pub certificate: String,
}

#[cfg(feature = "browserid")]
#[derive(Deserialize)]
pub struct KeysResponse {
    // ka: Vec<u8>,
    pub wrap_kb: Vec<u8>,
}

#[cfg(feature = "browserid")]
#[cfg(test)]
mod tests {
    use super::*;
    use ring::{digest, pbkdf2};

    fn quick_strech_pwd(email: &str, pwd: &str) -> Vec<u8> {
        let salt = kwe("quickStretch", email);
        let mut out = [0u8; 32];
        pbkdf2::derive(
            &digest::SHA256,
            std::num::NonZeroU32::new(1000).unwrap(),
            &salt,
            pwd.as_bytes(),
            &mut out,
        );
        out.to_vec()
    }

    fn auth_pwd(email: &str, pwd: &str) -> String {
        let streched = quick_strech_pwd(email, pwd);
        let salt = [0u8; 0];
        let context = kw("authPW");
        let derived = derive_hkdf_sha256_key(&streched, &salt, &context, 32);
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

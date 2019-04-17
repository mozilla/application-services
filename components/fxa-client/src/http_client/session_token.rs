/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    errors::*,
    http_client::{self, OAuthTokenResponse},
    util::Xorable,
    Config,
};
use hawk::{Credentials, Key, PayloadHasher, RequestBuilder, SHA256};
use ring::{digest, hkdf, hmac};
use serde_derive::*;
use serde_json::json;
use url::Url;
use viaduct::{header_names, Method, Request};

const HKDF_SALT: [u8; 32] = [0b0; 32];
const KEY_LENGTH: usize = 32;

pub struct HawkRequestBuilder<'a> {
    url: Url,
    method: Method,
    body: Option<String>,
    hkdf_sha256_key: &'a [u8],
}

impl<'a> HawkRequestBuilder<'a> {
    pub fn new(method: Method, url: Url, hkdf_sha256_key: &'a [u8]) -> Self {
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
            hash = PayloadHasher::hash("application/json", &SHA256, &body);
            hawk_request_builder = hawk_request_builder.hash(&hash[..]);
        }
        let hawk_request = hawk_request_builder.request();
        let token_id = hex::encode(&self.hkdf_sha256_key[0..KEY_LENGTH]);
        let hmac_key = &self.hkdf_sha256_key[KEY_LENGTH..(2 * KEY_LENGTH)];
        let hawk_credentials = Credentials {
            id: token_id,
            key: Key::new(hmac_key, &SHA256),
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

pub trait FxASessionTokenClient: http_client::FxAClient {
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
}

impl FxASessionTokenClient for http_client::Client {
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
        let parameters = json!({
            "client_id": config.client_id,
            "grant_type": "fxa-credentials",
            "scope": scopes.join(" ")
        });
        let key = derive_key_from_session_token(session_token)?;
        let url = config.token_endpoint()?;
        let request = HawkRequestBuilder::new(Method::Post, url, &key)
            .body(parameters)
            .build()?;
        Self::make_request(request)?.json().map_err(Into::into)
    }
}

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

pub fn derive_sync_key(kb: &[u8]) -> Vec<u8> {
    let salt = [0u8; 0];
    let context_info = kw("oldsync");
    derive_hkdf_sha256_key(&kb, &salt, &context_info, KEY_LENGTH * 2)
}

pub fn compute_client_state(kb: &[u8]) -> String {
    hex::encode(digest::digest(&digest::SHA256, &kb).as_ref()[0..16].to_vec())
}

fn derive_key_from_session_token(session_token: &[u8]) -> Result<Vec<u8>> {
    let context_info = kw("sessionToken");
    Ok(derive_hkdf_sha256_key(
        session_token,
        &HKDF_SALT,
        &context_info,
        KEY_LENGTH * 2,
    ))
}

fn derive_hkdf_sha256_key(ikm: &[u8], salt: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    let salt = hmac::SigningKey::new(&digest::SHA256, salt);
    let mut out = vec![0u8; len];
    hkdf::extract_and_expand(&salt, ikm, info, &mut out);
    out.to_vec()
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
pub struct KeysResponse {
    // ka: Vec<u8>,
    pub wrap_kb: Vec<u8>,
}

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

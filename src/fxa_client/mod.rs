use hex;
use hkdf::Hkdf;
use reqwest;
use reqwest::{Client, Method, Request};
use serde::Deserialize;
use serde_json;
use sha2::{Digest, Sha256};
use std;
use url::Url;

use self::browser_id::{jwt_utils, rsa, BrowserIDKeyPair, VerifyingPublicKey};
use self::browser_id::rsa::RSABrowserIDKeyPair;
use self::errors::*;
use self::hawk_request::FxAHAWKRequestBuilder;
use {FxAConfig};

pub mod browser_id;
pub mod errors;
mod hawk_request;

const HKDF_SALT: [u8; 32] = [0b0; 32];
const KEY_LENGTH: usize = 32;
const OAUTH_CLIENT_ID: &str = "5882386c6d801776"; // TODO: CHANGE ME!
const SIGN_DURATION_MS: u64 = 24 * 60 * 60 * 1000;

pub struct FxAClient<'a> {
  config: &'a FxAConfig
}

impl<'a> FxAClient<'a> {
  pub fn new(config: &'a FxAConfig) -> FxAClient<'a> {
    FxAClient {
      config
    }
  }

  fn kw(name: &str) -> Vec<u8> {
    format!("identity.mozilla.com/picl/v1/{}", name).as_bytes().to_vec()
  }

  fn kwe(name: &str, email: &str) -> Vec<u8> {
    format!("identity.mozilla.com/picl/v1/{}:{}", name, email).as_bytes().to_vec()
  }

  pub fn key_pair(len: u32) -> Result<RSABrowserIDKeyPair> {
    rsa::generate_keypair(len)
  }

  pub fn derive_sync_key(kb: &[u8]) -> Vec<u8> {
    let salt = [0u8; 0];
    let context_info = FxAClient::kw("oldsync");
    FxAClient::derive_hkdf_sha256_key(&kb, &salt, &context_info, KEY_LENGTH * 2)
  }

  pub fn compute_client_state(kb: &[u8]) -> String {
    hex::encode(&Sha256::digest(kb)[0..16])
  }

  pub fn sign_out(&self) {
    panic!("Not implemented yet!");
  }

  pub fn login(&self, email: &str, auth_pwd: &str, get_keys: bool) -> Result<LoginResponse> {
    let url = self.build_url(&self.config.auth_url, "account/login")?;
    let parameters = json!({
      "email": email,
      "authPW": auth_pwd
    });
    let client = Client::new();
    let request = client.request(Method::Post, url)
      .query(&[("keys", get_keys)])
      .body(parameters.to_string()).build()
      .chain_err(|| "Could not build request.")?;
    FxAClient::make_request(request)
  }

  pub fn account_status(&self, uid: &String) -> Result<AccountStatusResponse> {
    let url = self.build_url(&self.config.auth_url, "account/status")?;

    let client = Client::new();
    let request = client.get(url)
      .query(&[("uid", uid)]).build()
      .chain_err(|| "Could not build request.")?;
    FxAClient::make_request(request)
  }

  pub fn keys(&self, key_fetch_token: &[u8]) -> Result<KeysResponse> {
    let url = self.build_url(&self.config.auth_url, "account/keys")?;
    let context_info = FxAClient::kw("keyFetchToken");
    let key = FxAClient::derive_hkdf_sha256_key(&key_fetch_token, &HKDF_SALT, &context_info, KEY_LENGTH * 3);
    let request = FxAHAWKRequestBuilder::new(Method::Get, url, &key).build()?;
    FxAClient::make_request(request)
  }

  pub fn recovery_email_status(&self, session_token: &[u8]) -> Result<RecoveryEmailStatusResponse> {
    let url = self.build_url(&self.config.auth_url, "recovery_email/status")?;
    let key = FxAClient::derive_key_from_session_token(session_token)?;
    let request = FxAHAWKRequestBuilder::new(Method::Get, url, &key).build()?;
    FxAClient::make_request(request)
  }

  pub fn oauth_authorize(&self, session_token: &[u8], scope: &str) -> Result<OAuthAuthorizeResponse> {
    let audience = self.get_oauth_audience()?;
    let key_pair = FxAClient::key_pair(1024)
      .chain_err(|| "Could not create keypair.")?;
    let certificate = self.sign(session_token, key_pair.public_key())?.certificate;
    let assertion = jwt_utils::create_assertion(key_pair.private_key(), &certificate, &audience)
      .chain_err(|| "Could not generate assertion.")?;
    let parameters = json!({
      "assertion": assertion,
      "client_id": OAUTH_CLIENT_ID,
      "response_type": "token",
      "scope": scope
    });
    let key = FxAClient::derive_key_from_session_token(session_token)?;
    let url = self.build_url(&self.config.oauth_url, "authorization")?;
    let request = FxAHAWKRequestBuilder::new(Method::Post, url, &key)
      .body(parameters).build()?;
    FxAClient::make_request(request)
  }

  pub fn sign(&self, session_token: &[u8], public_key: &VerifyingPublicKey) -> Result<SignResponse> {
    let public_key_json = public_key.to_json()
      .chain_err(|| "Could not get public key json representation.")?;
    let parameters = json!({
      "publicKey": public_key_json,
      "duration": SIGN_DURATION_MS
    });
    let key = FxAClient::derive_key_from_session_token(session_token)?;
    let url = self.build_url(&self.config.auth_url, "certificate/sign")?;
    let request = FxAHAWKRequestBuilder::new(Method::Post, url, &key)
      .body(parameters).build()?;
    FxAClient::make_request(request)
  }

  fn get_oauth_audience(&self) -> Result<String> {
    let url = Url::parse(&self.config.oauth_url)
      .chain_err(|| "Could not parse base URL")?;
    let host = url.host_str()
      .chain_err(|| "Could get host")?;
    match url.port() {
      Some(port) => Ok(format!("{}://{}:{}", url.scheme(), host, port)),
      None => Ok(format!("{}://{}", url.scheme(), host))
    }
  }

  fn build_url(&self, base_url: &String, path: &str) -> Result<Url> {
    let base_url = Url::parse(base_url)
      .chain_err(|| "Could not parse base URL")?;
    base_url.join(path)
      .chain_err(|| "Could not append path")
  }

  fn derive_key_from_session_token(session_token: &[u8]) -> Result<Vec<u8>> {
    let context_info = FxAClient::kw("sessionToken");
    Ok(FxAClient::derive_hkdf_sha256_key(session_token, &HKDF_SALT, &context_info, KEY_LENGTH * 2))
  }

  fn derive_hkdf_sha256_key(ikm: &[u8], xts: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    let hk = Hkdf::<Sha256>::extract(&xts, &ikm);
    hk.expand(&info, len)
  }

  fn make_request<T>(request: Request) -> Result<T> where for<'de> T: Deserialize<'de> {
    let client = Client::new();
    let mut resp = client.execute(request)
      .chain_err(|| "Request failed")?;

    if resp.status().is_success() {
      resp.json().chain_err(|| "Deserialization failed")
    } else {
      let json: std::result::Result<serde_json:: Value, reqwest::Error> = resp.json();
      match json {
        Ok(json) => bail!(ErrorKind::RemoteError(
          json["code"].as_u64().unwrap_or(0),
          json["errno"].as_u64().unwrap_or(0),
          json["error"].as_str().unwrap_or("").to_string(),
          json["message"].as_str().unwrap_or("").to_string(),
          json["info"].as_str().unwrap_or("").to_string())),
        // This is a bit awkward: we wrap reqwest:Error in our Error type.
        Err(_) => Err(resp.error_for_status().unwrap_err())
                       .chain_err(|| "Request failed.")
      }
    }
  }
}

#[derive(Deserialize)]
pub struct LoginResponse {
  pub uid: String,
  #[serde(rename = "sessionToken")]
  pub session_token: String,
  pub verified: bool
}

#[derive(Deserialize)]
pub struct RecoveryEmailStatusResponse {
  pub email: String,
  pub verified: bool
}

#[derive(Deserialize)]
pub struct AccountStatusResponse {
  pub exists: bool
}

#[derive(Deserialize)]
pub struct OAuthAuthorizeResponse {
  pub access_token: String
}

#[derive(Deserialize)]
pub struct SignResponse {
  #[serde(rename = "cert")]
  pub certificate: String
}

#[derive(Deserialize)]
pub struct KeysResponse {
  // ka: Vec<u8>,
  pub wrap_kb: Vec<u8>
}

#[cfg(test)]
mod tests {
  extern crate ring;
  use super::*;
  use self::ring::{digest, pbkdf2};

  fn quick_strech_pwd(email: &str, pwd: &str) -> Vec<u8> {
    let salt = FxAClient::kwe("quickStretch", email);
    let mut out = [0u8; digest::SHA256_OUTPUT_LEN];
    pbkdf2::derive(&digest::SHA256, 1000, &salt, pwd.as_bytes(), &mut out);
    out.to_vec()
  }

  fn auth_pwd(email: &str, pwd: &str) -> String {
    let streched = quick_strech_pwd(email, pwd);
    let salt = [0u8; 0];
    let context = FxAClient::kw("authPW");
    let derived = FxAClient::derive_hkdf_sha256_key(&streched, &salt, &context, 32);
    hex::encode(derived)
  }

  #[test]
  fn test_quick_strech_pwd() {
    let email = "andré@example.org";
    let pwd = "pässwörd";
    let streched = hex::encode(quick_strech_pwd(email, pwd));
    assert_eq!(streched, "e4e8889bd8bd61ad6de6b95c059d56e7b50dacdaf62bd84644af7e2add84345d");
  }

  #[test]
  fn test_auth_pwd() {
    let email = "andré@example.org";
    let pwd = "pässwörd";
    let auth_pwd = auth_pwd(email, pwd);
    assert_eq!(auth_pwd, "247b675ffb4c46310bc87e26d712153abe5e1c90ef00a4784594f97ef54f2375");
  }

  #[test]
  fn live_account_test() {
    let email = "testfxarustclient@restmail.net";
    let pwd = "testfxarustclient@restmail.net";
    let auth_pwd = auth_pwd(email, pwd);

    let config = FxAConfig {
      auth_url: "https://stable.dev.lcip.org/auth/v1/".to_string(),
      oauth_url: "https://oauth-stable.dev.lcip.org/v1/".to_string(),
      profile_url: "https://stable.dev.lcip.org/profile/".to_string()
    };
    let client = FxAClient::new(&config);

    let resp = client.login(&email, &auth_pwd, false).unwrap();
    println!("Session Token obtained: {}", &resp.session_token);
    let session_token = hex::decode(resp.session_token).unwrap();

    let resp = client.oauth_authorize(&session_token, "profile").unwrap();
    println!("OAuth Token obtained: {}", &resp.access_token);
  }
}

use hex;
use hkdf::Hkdf;
use reqwest::{Client, Method, Request};
use serde::Deserialize;
use serde_json;
use sha2::Sha256;
use url::Url;

use self::browser_id::{jwt_utils, rsa, BrowserIDKeyPair, VerifyingPublicKey};
use self::errors::*;
use self::hawk_request::FxAHAWKRequestBuilder;
use {FxAConfig};

mod browser_id;
pub mod errors;
mod hawk_request;

const HKDF_SALT: [u8; 32] = [0b0; 32];
const KEY_LENGTH: usize = 32;
const OAUTH_CLIENT_ID: &str = "1b1a3e44c54fbb58"; // TODO: CHANGE ME!
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

  fn keyword(kw: &str) -> Vec<u8> {
    let kw = format!("identity.mozilla.com/picl/v1/{}", kw);
    kw.as_bytes().to_vec()
  }

  pub fn sign_out(&self) {
    panic!("Not implemented yet!");
  }

  pub fn account_status(&self, uid: &String) -> Result<AccountStatusResponse> {
    let url = self.build_url(&self.config.auth_url, "account/status")?;

    let client = Client::new();
    client.get(url)
      .query(&[("uid", uid)])
      .send().chain_err(|| ErrorKind::LocalError("Request failed".to_string()))?
      .json().chain_err(|| ErrorKind::LocalError("JSON parse failed".to_string()))
  }

  // pub fn keys(&self, key_fetch_token: &[u8]) -> Result<()> {
  //   let url = self.build_url(&self.config.auth_url, "account/keys")?;

  //   let context_info = FxAClient::keyword("keyFetchToken");
  //   let key = FxaClient::derive_hkdf_sha256_key(key_fetch_token, &HKDF_SALT, &context_info, KEY_LENGTH * 3);

  //   let request = FxAHAWKRequestBuilder::new(Method::Get, url, &key).build()?;
  //   let json: serde_json::Value = FxAClient::make_request(request)?;

  //   // Derive key from response.
  //   let key_request_key = &key[(2 * KEY_LENGTH)..(3 * KEY_LENGTH)];

  //   // let bundle = json.get("bundle")?;

  //   Ok(())
  // }

  pub fn recovery_email_status(&self, session_token: &String) -> Result<RecoveryEmailStatusResponse> {
    let url = self.build_url(&self.config.auth_url, "recovery_email/status")?;

    let key = FxAClient::derive_key_from_session_token(session_token)?;
    let request = FxAHAWKRequestBuilder::new(Method::Get, url, &key).build()?;
    FxAClient::make_request(request)
  }

  pub fn oauth_authorize(&self, session_token: &String, scope: &str) -> Result<OAuthResponse> {
    let audience = self.get_oauth_audience()?;
    let key_pair = rsa::generate_keypair(1024)
      .chain_err(|| ErrorKind::LocalError("Could not create keypair.".to_string()))?;
    let private_key = key_pair.private_key();
    let certificate = self.sign(session_token, key_pair.public_key())?.certificate;
    let assertion = jwt_utils::create_assertion(private_key, certificate, audience)
      .chain_err(|| ErrorKind::LocalError("Could not generate assertion.".to_string()))?;
    let parameters = json!({
      "assertion": assertion,
      "client_id": OAUTH_CLIENT_ID,
      "response_type": "token",
      "scope": scope
    });
    let key = FxAClient::derive_key_from_session_token(session_token)?;
    let url = self.build_url(&self.config.oauth_url, "/authorization")?;
    let request = FxAHAWKRequestBuilder::new(Method::Post, url, &key)
      .body(parameters).build()?;
    FxAClient::make_request(request)
  }

  pub fn sign(&self, session_token: &String, public_key: &VerifyingPublicKey) -> Result<SignResponse> {
    let parameters = json!({
      "publicKey": public_key.to_json(),
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
      .chain_err(|| ErrorKind::LocalError("Could not parse base URL".to_string()))?;
    let host = url.host_str()
      .chain_err(|| ErrorKind::LocalError("Could get host".to_string()))?;
    match url.port() {
      Some(port) => Ok(format!("{}://{}:{}", url.scheme(), host, port)),
      None => Ok(format!("{}://{}", url.scheme(), host))
    }
  }

  fn build_url(&self, base_url: &String, path: &str) -> Result<Url> {
    let base_url = Url::parse(base_url)
      .chain_err(|| ErrorKind::LocalError("Could not parse base URL".to_string()))?;
    base_url.join(path)
      .chain_err(|| ErrorKind::LocalError("Could not append path".to_string()))
  }

  fn derive_key_from_session_token(session_token: &String) -> Result<Vec<u8>> {
    let context_info = FxAClient::keyword("sessionToken");
    let session_token = hex::decode(session_token)
      .chain_err(|| ErrorKind::LocalError("Could not decode session token".to_string()))?;
    Ok(FxAClient::derive_hkdf_sha256_key(&session_token, &HKDF_SALT, &context_info, KEY_LENGTH * 2))
  }

  fn derive_hkdf_sha256_key(ikm: &[u8], xts: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    let hk = Hkdf::<Sha256>::extract(&ikm, &xts);
    hk.expand(&info, len)
  }

  fn make_request<T>(request: Request) -> Result<T> where for<'de> T: Deserialize<'de> {
    let client = Client::new();
    let mut resp = client.execute(request)
      .chain_err(|| ErrorKind::LocalError("Request failed".to_string()))?;

    let json: serde_json::Value = resp.json()
      .chain_err(|| ErrorKind::LocalError("JSON parse failed".to_string()))?;

    if resp.status().is_success() {
      resp.json().chain_err(|| ErrorKind::LocalError("JSON parse failed".to_string()))
    } else {
      bail!(ErrorKind::RemoteError(json["code"].as_u64().unwrap_or(0),
                                   json["errno"].as_u64().unwrap_or(0),
                                   json["error"].as_str().unwrap_or("").to_string(),
                                   json["message"].as_str().unwrap_or("").to_string(),
                                   json["info"].as_str().unwrap_or("").to_string()))
    }
  }
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
pub struct OAuthResponse {
  #[serde(rename = "accessToken")]
  pub access_token: String
}

#[derive(Deserialize)]
pub struct SignResponse {
  certificate: String
}

// #[cfg(test)]
// mod tests {
//   use super::*;

//   #[test]
//   fn it_works() {
//     let config = FxAConfig {
//       auth_url: "https://api.accounts.firefox.com/v1/".to_string(),
//       oauth_url: "https://oauth.accounts.firefox.com/v1/".to_string(),
//       profile_url: "https://profile.accounts.firefox.com/v1/".to_string()
//     };
//     let client = FxAClient::new(&config);
//     let key_fetch_token: &[u8] = &[0b0; KEY_LENGTH];
//     client.keys(key_fetch_token).expect("did not work!");
//   }
// }

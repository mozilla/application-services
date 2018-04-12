use error::*;
use super::FxAConfig;
use super::crypto;
use super::hawk_request::FxAHAWKRequestBuilder;

use url::Url;
use reqwest::{Client, Method};
use serde_json::Value;
use hex;

const HKDF_SALT: [u8; 32] = [0b0; 32];
const KEY_LENGTH: usize = 32;

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

  pub fn keys(&self, key_fetch_token: &[u8]) -> Result<()> {
    let base_url = Url::parse(&self.config.auth_url)
      .chain_err(|| ErrorKind::LocalError("Could not parse base URL".to_string()))?;
    let url = base_url.join("account/keys")
      .chain_err(|| ErrorKind::LocalError("Could not append path".to_string()))?;

    let context_info = FxAClient::keyword("keyFetchToken");
    let key = crypto::derive_hkdf_sha256_key(key_fetch_token, &HKDF_SALT, &context_info, KEY_LENGTH * 3);

    let request = FxAHAWKRequestBuilder::new(Method::Get, url, &key).build()?;

    let client = Client::new();
    let mut resp = client.execute(request)
      .chain_err(|| ErrorKind::RemoteError("Request failed".to_string()))?;
    let json: Value = resp.json()
      .chain_err(|| ErrorKind::LocalError("JSON parse failed".to_string()))?;

    // Derive key from response.
    let key_request_key = &key[(2 * KEY_LENGTH)..(3 * KEY_LENGTH)];

    // let bundle = json.get("bundle")?;

    Ok(())
  }

  pub fn recovery_email_status(&self, session_token: &String) -> Result<RecoveryEmailStatusResponse> {
    let base_url = Url::parse(&self.config.auth_url)
      .chain_err(|| ErrorKind::LocalError("Could not parse base URL".to_string()))?;
    let url = base_url.join("recovery_email/status")
      .chain_err(|| ErrorKind::LocalError("Could not append path".to_string()))?;

    let context_info = FxAClient::keyword("sessionToken");
    let session_token = hex::decode(session_token)
      .chain_err(|| ErrorKind::LocalError("Could not decode session token".to_string()))?;
    let key = crypto::derive_hkdf_sha256_key(&session_token, &HKDF_SALT, &context_info, KEY_LENGTH * 2);

    let request = FxAHAWKRequestBuilder::new(Method::Get, url, &key).build()?;

    let client = Client::new();
    let mut resp = client.execute(request)
      .chain_err(|| ErrorKind::RemoteError("Request failed".to_string()))?;

    resp.json().chain_err(|| ErrorKind::LocalError("JSON parse failed".to_string()))
  }
}

#[derive(Deserialize)]
pub struct RecoveryEmailStatusResponse {
  pub email: String,
  pub verified: bool
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

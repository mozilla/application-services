use error::*;
use super::crypto;
use super::hawk_request::FxAHAWKRequestBuilder;

use url::Url;
use reqwest::{Client, Method};
use serde_json::Value;

const HKDF_SALT: [u8; 32] = [0b0; 32];
const KEY_LENGTH: usize = 32;

struct FxAClient {
  auth_url: Url, // Needs a trailing slash if a path is part of the root URL!
  oauth_url: Url,
  profile_url: Url
}

impl FxAClient {
  fn keyword(kw: &str) -> Vec<u8> {
    let kw = format!("identity.mozilla.com/picl/v1/{}", kw);
    kw.as_bytes().to_vec()
  }

  pub fn keys(&self, key_fetch_token: &[u8]) -> Result<()> {
    let url = self.auth_url.join("account/keys")
      .chain_err(|| ErrorKind::LocalError("Could append path".to_string()))?;

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
}

// #[cfg(test)]
// mod tests {
//   use super::*;

//   #[test]
//   fn it_works() {
//     let auth_url = Url::parse("https://api.accounts.firefox.com/v1/").expect("Parsing failed.");
//     let oauth_url = Url::parse("https://oauth.accounts.firefox.com/v1/").expect("Parsing failed.");
//     let profile_url = Url::parse("https://profile.accounts.firefox.com/v1/").expect("Parsing failed.");
//     let client = FxAClient {
//       auth_url,
//       oauth_url,
//       profile_url
//     };
//     let key_fetch_token: &[u8] = &[0b0; KEY_LENGTH];
//     client.keys(key_fetch_token).expect("did not work!");
//   }
// }

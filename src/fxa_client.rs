use error::*;
use super::crypto;
use super::hawk_request::FxAHAWKRequestBuilder;

use url::Url;
use reqwest::{Client, Method};

const HKDF_SALT: [u8; 32] = [0b0; 32];
const KEY_LENGTH: usize = 32;

struct FxAClient {
  auth_url: Url,
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

    let request = FxAHAWKRequestBuilder::new(Method::Get, url, key).build()?;

    let client = Client::new();
    let resp = client.execute(request);

    println!("{}", resp.is_ok());

    Ok(())
  }
}

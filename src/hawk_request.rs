use error::*;

use hawk;
use hawk::{Credentials, Key, SHA256, PayloadHasher};
use reqwest;
use reqwest::{Request, Method};
use url::Url;
use hex;

const KEY_LENGTH: usize = 32;

pub struct FxAHAWKRequestBuilder<'a> {
  url: Url,
  method: Method,
  body: Option<Vec<u8>>,
  hkdf_sha256_key: &'a Vec<u8>,
}

impl<'a> FxAHAWKRequestBuilder<'a> {
  pub fn new(method: Method, url: Url, hkdf_sha256_key: &'a Vec<u8>) -> Self {
    FxAHAWKRequestBuilder {
      url: url,
      method: method,
      body: None,
      hkdf_sha256_key: hkdf_sha256_key,
    }
  }

  // This class assumes that the content being sent it always of the type
  // application/json.
  pub fn body(mut self, body: Vec<u8>) -> Self {
    self.body = Some(body);
    self
  }

  pub fn build(self) -> Result<Request> {
    // Make sure we de-allocate the hash after hawk_request_builder.
    let hash;
    let method = format!("{}", self.method);
    let mut hawk_request_builder = hawk::RequestBuilder::from_url(method.as_str(), &self.url)
      .chain_err(|| ErrorKind::LocalError("Could not parse URL".to_string()))?;
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
    let hawk_header = hawk_request.make_header(&hawk_credentials)
      .chain_err(|| ErrorKind::LocalError("Could not create hawk header".to_string()))?;
    let hawk_header = format!("Hawk {}", hawk_header);

    let mut request_builder = reqwest::Client::new()
      .request(self.method, self.url.as_str());
    request_builder.header(reqwest::header::Authorization(hawk_header));

    if let Some(body) = self.body {
      request_builder.header(reqwest::header::ContentType::json());
      request_builder.body(body);
    }

    Ok(request_builder.build()
        .chain_err(|| ErrorKind::LocalError("Could not create request".to_string()))?)
  }
}

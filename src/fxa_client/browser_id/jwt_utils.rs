use std::time::{SystemTime, UNIX_EPOCH};

use base64;
use errors::*;
use serde_json;

use fxa_client::browser_id::SigningPrivateKey;

const DEFAULT_ASSERTION_ISSUER: &str = "127.0.0.1";
const DEFAULT_ASSERTION_DURATION: u64 = 60 * 60 * 1000;

pub fn create_assertion(private_key: &SigningPrivateKey, certificate: String,
                        audience: String) -> Result<String> {
  let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH)
    .expect("Something is very wrong.");
  let issued_at = since_epoch.as_secs() * 1000 +
                  since_epoch.subsec_nanos() as u64 / 1_000_000;
  let expires_at = issued_at + DEFAULT_ASSERTION_DURATION;
  let builder = SignedJWTBuilder::new(private_key, DEFAULT_ASSERTION_ISSUER.to_string(),
                                      issued_at, expires_at);
  builder.build()
}

struct SignedJWTBuilder<'a> {
  private_key: &'a SigningPrivateKey,
  issuer: String,
  issued_at: u64,
  expires_at: u64,
  audience: Option<String>,
  payload: Option<serde_json::Value>
}

impl<'a> SignedJWTBuilder<'a> {
  fn new(private_key: &'a SigningPrivateKey, issuer: String,
         issued_at: u64, expires_at: u64) -> SignedJWTBuilder<'a> {
    SignedJWTBuilder {
      private_key,
      issuer,
      issued_at,
      expires_at,
      audience: None,
      payload: None
    }
  }

  fn audience(mut self, audience: &str) -> SignedJWTBuilder<'a> {
    self.audience = Some(audience.to_string());
    self
  }

  fn payload(mut self, payload: serde_json::Value) -> SignedJWTBuilder<'a> {
    self.payload = Some(payload);
    self
  }

  fn build(self) -> Result<String> {
    let payload_string = self.get_payload_string()?;
    SignedJWTBuilder::encode(self.private_key, &payload_string)
  }

  fn get_payload_string(&self) -> Result<String> {
    let mut payload = match self.payload {
      Some(ref payload) => payload.clone(),
      None => json!({})
    };
    let obj = match payload.as_object_mut() {
      Some(obj) => obj,
      None => bail!("Not an object!")
    };
    if let Some(ref audience) = self.audience {
      obj.insert("aud".to_string(), json!(audience));
    }
    obj.insert("iss".to_string(), json!(self.issuer));
    obj.insert("iat".to_string(), json!(self.issued_at));
    obj.insert("exp".to_string(), json!(self.expires_at));
    Ok(json!(obj).to_string())
  }

  fn encode(private_key: &SigningPrivateKey, payload: &String) -> Result<String> {
    let headers_str = json!({"alg": private_key.get_algo()}).to_string();
    let encoded_header = base64::encode_config(headers_str.as_bytes(), base64::URL_SAFE_NO_PAD);
    let encoded_payload = base64::encode_config(payload.as_bytes(), base64::URL_SAFE_NO_PAD);
    let message = format!("{}.{}", encoded_header, encoded_payload);
    let signature = private_key.sign(message.as_bytes())?;
    let encoded_signature = base64::encode_config(&signature, base64::URL_SAFE_NO_PAD);
    Ok(format!("{}.{}", message, encoded_signature))
  }
}

use std::time::{SystemTime, UNIX_EPOCH};

use base64;
use errors::*;
use serde_json;

use fxa_client::browser_id::{SigningPrivateKey, VerifyingPublicKey};

const DEFAULT_ASSERTION_ISSUER: &str = "127.0.0.1";
const DEFAULT_ASSERTION_DURATION: u64 = 60 * 60 * 1000;

pub fn create_assertion(private_key: &SigningPrivateKey, certificate: &str,
                        audience: &str) -> Result<String> {
  let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH)
    .expect("Something is very wrong.");
  let issued_at = since_epoch.as_secs() * 1000 +
                  since_epoch.subsec_nanos() as u64 / 1_000_000;
  let expires_at = issued_at + DEFAULT_ASSERTION_DURATION;
  let issuer = DEFAULT_ASSERTION_ISSUER;
  create_assertion_full(private_key, certificate, audience, issuer, issued_at, expires_at)
}

pub fn create_assertion_full(private_key: &SigningPrivateKey, certificate: &str,
                             audience: &str, issuer: &str,
                             issued_at: u64, expires_at: u64) -> Result<String> {
  let assertion = SignedJWTBuilder::new(private_key, issuer, issued_at, expires_at)
    .audience(&audience)
    .build()
    .chain_err(|| "Could not build assertion.")?;
  Ok(format!("{}~{}", &certificate, &assertion))
}

struct SignedJWTBuilder<'a> {
  private_key: &'a SigningPrivateKey,
  issuer: &'a str,
  issued_at: u64,
  expires_at: u64,
  audience: Option<&'a str>,
  payload: Option<serde_json::Value>
}

impl<'a> SignedJWTBuilder<'a> {
  fn new(private_key: &'a SigningPrivateKey, issuer: &'a str,
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

  fn audience(mut self, audience: &'a str) -> SignedJWTBuilder<'a> {
    self.audience = Some(audience);
    self
  }

  fn payload(mut self, payload: serde_json::Value) -> SignedJWTBuilder<'a> {
    self.payload = Some(payload);
    self
  }

  fn build(self) -> Result<String> {
    let payload_string = self.get_payload_string()?;
    encode(&payload_string, self.private_key)
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
}

fn encode(payload: &str, private_key: &SigningPrivateKey) -> Result<String> {
  let headers_str = json!({"alg": private_key.get_algo()}).to_string();
  let encoded_header = base64::encode_config(headers_str.as_bytes(), base64::URL_SAFE_NO_PAD);
  let encoded_payload = base64::encode_config(payload.as_bytes(), base64::URL_SAFE_NO_PAD);
  let message = format!("{}.{}", encoded_header, encoded_payload);
  let signature = private_key.sign(message.as_bytes())?;
  let encoded_signature = base64::encode_config(&signature, base64::URL_SAFE_NO_PAD);
  Ok(format!("{}.{}", message, encoded_signature))
}

fn decode(token: &str, public_key: &VerifyingPublicKey) -> Result<String> {
  let segments: Vec<&str> = token.split(".").collect();
  let message = format!("{}.{}", &segments[0], &segments[1]);
  let message_bytes = message.as_bytes();
  let signature = base64::decode_config(&segments[2], base64::URL_SAFE_NO_PAD)
    .chain_err(|| "Could not decode base64 signature.")?;
  let verified = public_key.verify_message(&message_bytes, &signature)
    .chain_err(|| "Could not verify message.")?;
  if !verified {
    bail!("Could not verify token.")
  }
  let payload = base64::decode_config(&segments[1], base64::URL_SAFE_NO_PAD)
    .chain_err(|| "Could not decode base64 payload.")?;
  String::from_utf8(payload).chain_err(|| "Could not decode UTF-8 payload.")
}

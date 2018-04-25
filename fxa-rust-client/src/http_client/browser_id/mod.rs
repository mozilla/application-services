use serde_json;

use http_client::errors::*;

pub mod jwt_utils;
pub mod rsa;

pub trait BrowserIDKeyPair {
  fn private_key(&self) -> &SigningPrivateKey;
  fn public_key(&self) -> &VerifyingPublicKey;
}

pub trait SigningPrivateKey {
  fn get_algo(&self) -> String;
  fn sign(&self, message: &[u8]) -> Result<Vec<u8>>;
}

pub trait VerifyingPublicKey {
  fn verify_message(&self, message: &[u8], signature: &[u8]) -> Result<bool>;
  fn to_json(&self) -> Result<serde_json::Value>;
}

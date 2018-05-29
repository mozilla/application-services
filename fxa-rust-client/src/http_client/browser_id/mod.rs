use serde_json;

use errors::*;

pub mod jwt_utils;
pub mod rsa;

pub trait BrowserIDKeyPair {
    fn get_algo(&self) -> String;
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>>;
    fn verify_message(&self, message: &[u8], signature: &[u8]) -> Result<bool>;
    fn to_json(&self, include_private: bool) -> Result<serde_json::Value>;
}

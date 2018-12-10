/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::errors::*;
pub mod jwt_utils;
pub mod rsa;

pub trait BrowserIDKeyPair {
    fn get_algo(&self) -> String;
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>>;
    fn verify_message(&self, message: &[u8], signature: &[u8]) -> Result<bool>;
    fn to_json(&self, include_private: bool) -> Result<serde_json::Value>;
}

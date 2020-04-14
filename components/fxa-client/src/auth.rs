/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{error::*, http_client::Client, FirefoxAccount, Config, util};
use crate::util::Xorable;
use rc_crypto::{digest, hkdf, hmac};
use serde_json::json;
use viaduct::Request;

// impl FirefoxAccount {
//     /// Log-in without any user interaction.
//     /// This method is only meant to be used by tests.
//     pub fn login(email: &str, password: &str) -> Result<()> {
//         Ok(())
//     }
// }

// impl Client {
//     fn account_login(
//         &self,
//         config: &Config,
//         email: &str,
//         auth_pw: &str,
//     ) -> Result<> {
//         let body = json!({
//             "email": email,
//             "authPW": auth_pw,
//             "service": &config.client_id,
//             "verificationMethod": "email-otp",
//         });
//         let url = config.introspection_endpoint()?;
//         Ok(Self::make_request(Request::post(url).json(&body))?.json()?)
//     }
// }

pub fn kwe(name: &str, email: &str) -> Vec<u8> {
    format!("identity.mozilla.com/picl/v1/{}:{}", name, email)
        .as_bytes()
        .to_vec()
}

pub fn kw(name: &str) -> Vec<u8> {
    format!("identity.mozilla.com/picl/v1/{}", name)
        .as_bytes()
        .to_vec()
}

pub fn derive_hkdf_sha256_key(ikm: &[u8], salt: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    let salt = hmac::SigningKey::new(&digest::SHA256, salt);
    let mut out = vec![0u8; len];
    hkdf::extract_and_expand(&salt, ikm, info, &mut out).unwrap();
    out
}

pub fn quick_strech_pwd(email: &str, pwd: &str) -> Vec<u8> {
    let salt = kwe("quickStretch", email);
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2::<::hmac::Hmac<sha2::Sha256>>(pwd.as_bytes(), &salt, 1000, &mut out);
    out.to_vec()
}

pub fn auth_pwd(email: &str, pwd: &str) -> String {
    let streched = quick_strech_pwd(email, pwd);
    let salt = [0u8; 0];
    let context = kw("authPW");
    let derived = derive_hkdf_sha256_key(&streched, &salt, &context, 32);
    hex::encode(derived)
}

pub fn random_code_challenge() -> Result<String> {
    let code_verifier = util::random_base64_url_string(43)?;
    let code_challenge = digest::digest(&digest::SHA256, &code_verifier.as_bytes())?;
    Ok(base64::encode_config(&code_challenge, base64::URL_SAFE_NO_PAD))
}

pub fn random_state() -> Result<String> {
    util::random_base64_url_string(16)
}

pub fn derive_hawk_credentials(token_hex: &str, context: &str, size: usize) -> Result<serde_json::Value> {
    let token = hex::decode(token_hex)?;
    let out = derive_hkdf_sha256_key(&token, &[0u8; 0], &kw(context), size);
    let key = match std::str::from_utf8(&out[32..64]) {
        Ok(v) => v,
        Err(e) => panic!("hawk_credentials.key: {}", e),
    };
    let extra = match std::str::from_utf8(&out[64..]) {
        Ok(v) => v,
        Err(e) => panic!("hawk_credentials.extra: {}", e),
    };
    Ok(json!({
        "key": key,
        "id": hex::encode(&out[0..32]),
        "extra": extra,
    }))
}

pub fn xored(a: &[u8], b: &[u8]) -> Result<Vec<u8>> {
    a.xored_with(b)
}

fn derive_unwrap_kb(email: &str, pwd: &str) -> Vec<u8> {
    let streched_pw = quick_strech_pwd(email, pwd);
    let out = derive_hkdf_sha256_key(&streched_pw, &[0b0; 32], &kw("mainKDF"), 64);
    out[32..64].to_vec()
}

pub fn derive_sync_key(email: &str, pwd: &str, wrap_kb: &[u8]) -> Result<Vec<u8>> {
    let unwrap_kb = derive_unwrap_kb(email, pwd);
    let kb = xored(wrap_kb, &unwrap_kb)?;
    Ok(derive_hkdf_sha256_key(
        &kb,
        &[0u8; 0],
        "identity.mozilla.com/picl/v1/oldsync".as_bytes(),
        64
    ))
}
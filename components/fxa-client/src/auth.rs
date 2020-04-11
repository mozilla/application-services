/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{error::*, http_client::Client, FirefoxAccount, Config};
use rc_crypto::{digest, hkdf, hmac};
use serde_json::json;
use viaduct::Request;

impl FirefoxAccount {
    /// Log-in without any user interaction.
    /// This method is only meant to be used by tests.
    pub fn login(email: &str, password: &str) -> Result<()> {
        Ok(())
    }
}

impl Client {
    fn account_login(
        &self,
        config: &Config,
        email: &str,
        auth_pw: &str,
    ) -> Result<> {
        let body = json!({
            "email": email,
            "authPW": auth_pw,
            "service": &config.client_id,
            "verificationMethod": "email-otp",
        });
        let url = config.introspection_endpoint()?;
        Ok(Self::make_request(Request::post(url).json(&body))?.json()?)
    }
}

fn kwe(name: &str, email: &str) -> Vec<u8> {
    format!("identity.mozilla.com/picl/v1/{}:{}", name, email)
        .as_bytes()
        .to_vec()
}

fn kw(name: &str) -> Vec<u8> {
    format!("identity.mozilla.com/picl/v1/{}", name)
        .as_bytes()
        .to_vec()
}

fn derive_hkdf_sha256_key(ikm: &[u8], salt: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    let salt = hmac::SigningKey::new(&digest::SHA256, salt);
    let mut out = vec![0u8; len];
    hkdf::extract_and_expand(&salt, ikm, info, &mut out).unwrap();
    out
}

fn quick_strech_pwd(email: &str, pwd: &str) -> Vec<u8> {
    let salt = kwe("quickStretch", email);
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2::<::hmac::Hmac<sha2::Sha256>>(pwd.as_bytes(), &salt, 1000, &mut out);
    out.to_vec()
}

fn auth_pwd(email: &str, pwd: &str) -> String {
    let streched = quick_strech_pwd(email, pwd);
    let salt = [0u8; 0];
    let context = kw("authPW");
    let derived = derive_hkdf_sha256_key(&streched, &salt, &context, 32);
    hex::encode(derived)
}

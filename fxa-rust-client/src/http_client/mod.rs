use hex;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use reqwest;
use reqwest::{header, Client, Method, Request};
use serde::Deserialize;
use serde_json;
use sha2::{Digest, Sha256};
use std;
use util::Xorable;

use self::browser_id::rsa::RSABrowserIDKeyPair;
use self::browser_id::{jwt_utils, rsa, BrowserIDKeyPair, VerifyingPublicKey};
use self::hawk_request::FxAHAWKRequestBuilder;
use config::Config;
use errors::*;

pub mod browser_id;
mod hawk_request;

const HKDF_SALT: [u8; 32] = [0b0; 32];
const KEY_LENGTH: usize = 32;
const SIGN_DURATION_MS: u64 = 24 * 60 * 60 * 1000;

pub struct FxAClient<'a> {
    config: &'a Config,
}

impl<'a> FxAClient<'a> {
    pub fn new(config: &'a Config) -> FxAClient<'a> {
        FxAClient { config }
    }

    fn kw(name: &str) -> Vec<u8> {
        format!("identity.mozilla.com/picl/v1/{}", name)
            .as_bytes()
            .to_vec()
    }

    fn kwe(name: &str, email: &str) -> Vec<u8> {
        format!("identity.mozilla.com/picl/v1/{}:{}", name, email)
            .as_bytes()
            .to_vec()
    }

    pub fn key_pair(len: u32) -> Result<RSABrowserIDKeyPair> {
        rsa::generate_keypair(len)
    }

    pub fn derive_sync_key(kb: &[u8]) -> Vec<u8> {
        let salt = [0u8; 0];
        let context_info = FxAClient::kw("oldsync");
        FxAClient::derive_hkdf_sha256_key(&kb, &salt, &context_info, KEY_LENGTH * 2)
    }

    pub fn compute_client_state(kb: &[u8]) -> String {
        hex::encode(&Sha256::digest(kb)[0..16])
    }

    pub fn sign_out(&self) {
        panic!("Not implemented yet!");
    }

    pub fn login(&self, email: &str, auth_pwd: &str, get_keys: bool) -> Result<LoginResponse> {
        let url = self.config.auth_url_path("v1/account/login")?;
        let parameters = json!({
      "email": email,
      "authPW": auth_pwd
    });
        let client = Client::new();
        let request = client
            .request(Method::Post, url)
            .query(&[("keys", get_keys)])
            .body(parameters.to_string())
            .build()?;
        FxAClient::make_request(request)
    }

    pub fn account_status(&self, uid: &String) -> Result<AccountStatusResponse> {
        let url = self.config.auth_url_path("v1/account/status")?;
        let client = Client::new();
        let request = client.get(url).query(&[("uid", uid)]).build()?;
        FxAClient::make_request(request)
    }

    pub fn keys(&self, key_fetch_token: &[u8]) -> Result<KeysResponse> {
        let url = self.config.auth_url_path("v1/account/keys")?;
        let context_info = FxAClient::kw("keyFetchToken");
        let key = FxAClient::derive_hkdf_sha256_key(
            &key_fetch_token,
            &HKDF_SALT,
            &context_info,
            KEY_LENGTH * 3,
        );
        let key_request_key = &key[(KEY_LENGTH * 2)..(KEY_LENGTH * 3)];
        let request = FxAHAWKRequestBuilder::new(Method::Get, url, &key).build()?;
        let json: serde_json::Value = FxAClient::make_request(request)?;
        let bundle = match json["bundle"].as_str() {
            Some(bundle) => bundle,
            None => bail!("Invalid JSON"),
        };
        let data = hex::decode(bundle)?;
        if data.len() != 3 * KEY_LENGTH {
            bail!("Data is not of the expected size.");
        }
        let ciphertext = &data[0..(KEY_LENGTH * 2)];
        let mac_code = &data[(KEY_LENGTH * 2)..(KEY_LENGTH * 3)];
        let context_info = FxAClient::kw("account/keys");
        let bytes = FxAClient::derive_hkdf_sha256_key(
            key_request_key,
            &HKDF_SALT,
            &context_info,
            KEY_LENGTH * 3,
        );
        let hmac_key = &bytes[0..KEY_LENGTH];
        let xor_key = &bytes[KEY_LENGTH..(KEY_LENGTH * 3)];

        let mut mac = match Hmac::<Sha256>::new_varkey(hmac_key) {
            Ok(mac) => mac,
            Err(_) => bail!("Could not create MAC key."),
        };
        mac.input(ciphertext);
        if let Err(_) = mac.verify(&mac_code) {
            bail!("Bad HMAC!");
        }

        let xored_bytes = ciphertext.xored_with(xor_key)?;
        let wrap_kb = xored_bytes[KEY_LENGTH..(KEY_LENGTH * 2)].to_vec();
        Ok(KeysResponse { wrap_kb })
    }

    pub fn recovery_email_status(
        &self,
        session_token: &[u8],
    ) -> Result<RecoveryEmailStatusResponse> {
        let url = self.config.auth_url_path("v1/recovery_email/status")?;
        let key = FxAClient::derive_key_from_session_token(session_token)?;
        let request = FxAHAWKRequestBuilder::new(Method::Get, url, &key).build()?;
        FxAClient::make_request(request)
    }

    pub fn profile(&self, profile_access_token: &str) -> Result<ProfileResponse> {
        let url = self.config.profile_url_path("v1/profile")?;
        let client = Client::new();
        let request = client
            .request(Method::Get, url)
            .header(header::Authorization(header::Bearer {
                token: profile_access_token.to_string(),
            }))
            .build()?;
        FxAClient::make_request(request)
    }

    pub fn oauth_token_with_assertion(
        &self,
        client_id: &str,
        session_token: &[u8],
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse> {
        let audience = self.get_oauth_audience()?;
        let key_pair = FxAClient::key_pair(1024)?;
        let certificate = self.sign(session_token, key_pair.public_key())?.certificate;
        let assertion =
            jwt_utils::create_assertion(key_pair.private_key(), &certificate, &audience)?;
        let parameters = json!({
          "assertion": assertion,
          "client_id": client_id,
          "response_type": "token",
          "scope": scopes.join(" ")
        });
        let key = FxAClient::derive_key_from_session_token(session_token)?;
        let url = self.config.oauth_url_path("v1/authorization")?;
        let request = FxAHAWKRequestBuilder::new(Method::Post, url, &key)
            .body(parameters)
            .build()?;
        FxAClient::make_request(request)
    }

    pub fn oauth_token_with_code(
        &self,
        code: &str,
        code_verifier: &str,
        client_id: &str,
    ) -> Result<OAuthTokenResponse> {
        let body = json!({
            "code": code,
            "client_id": client_id,
            "code_verifier": code_verifier
        });
        self.make_oauth_token_request(body)
    }

    pub fn oauth_token_with_refresh_token(
        &self,
        client_id: &str,
        refresh_token: &str,
        scopes: &[&str],
    ) -> Result<OAuthTokenResponse> {
        let body = json!({
            "grant_type": "refresh_token",
            "client_id": client_id,
            "refresh_token": refresh_token,
            "scope": scopes.join(" ")
        });
        self.make_oauth_token_request(body)
    }

    fn make_oauth_token_request(&self, body: serde_json::Value) -> Result<OAuthTokenResponse> {
        let url = self.config.oauth_url_path("v1/token")?;
        let client = Client::new();
        let request = client
            .request(Method::Post, url)
            .header(header::ContentType::json())
            .body(body.to_string())
            .build()?;
        FxAClient::make_request(request)
    }

    pub fn sign(
        &self,
        session_token: &[u8],
        public_key: &VerifyingPublicKey,
    ) -> Result<SignResponse> {
        let public_key_json = public_key.to_json()?;
        let parameters = json!({
      "publicKey": public_key_json,
      "duration": SIGN_DURATION_MS
    });
        let key = FxAClient::derive_key_from_session_token(session_token)?;
        let url = self.config.auth_url_path("v1/certificate/sign")?;
        let request = FxAHAWKRequestBuilder::new(Method::Post, url, &key)
            .body(parameters)
            .build()?;
        FxAClient::make_request(request)
    }

    fn get_oauth_audience(&self) -> Result<String> {
        let url = self.config.oauth_url()?;
        let host = url.host_str()
            .chain_err(|| "This URL doesn't have a host!")?;
        match url.port() {
            Some(port) => Ok(format!("{}://{}:{}", url.scheme(), host, port)),
            None => Ok(format!("{}://{}", url.scheme(), host)),
        }
    }

    fn derive_key_from_session_token(session_token: &[u8]) -> Result<Vec<u8>> {
        let context_info = FxAClient::kw("sessionToken");
        Ok(FxAClient::derive_hkdf_sha256_key(
            session_token,
            &HKDF_SALT,
            &context_info,
            KEY_LENGTH * 2,
        ))
    }

    fn derive_hkdf_sha256_key(ikm: &[u8], xts: &[u8], info: &[u8], len: usize) -> Vec<u8> {
        let hk = Hkdf::<Sha256>::extract(&xts, &ikm);
        hk.expand(&info, len)
    }

    fn make_request<T>(request: Request) -> Result<T>
    where
        for<'de> T: Deserialize<'de>,
    {
        let client = Client::new();
        let mut resp = client.execute(request)?;

        if resp.status().is_success() {
            Ok(resp.json()?)
        } else {
            let json: std::result::Result<serde_json::Value, reqwest::Error> = resp.json();
            match json {
                Ok(json) => bail!(ErrorKind::RemoteError(
                    json["code"].as_u64().unwrap_or(0),
                    json["errno"].as_u64().unwrap_or(0),
                    json["error"].as_str().unwrap_or("").to_string(),
                    json["message"].as_str().unwrap_or("").to_string(),
                    json["info"].as_str().unwrap_or("").to_string()
                )),
                Err(_) => Err(resp.error_for_status().unwrap_err().into()),
            }
        }
    }
}

#[derive(Deserialize)]
pub struct LoginResponse {
    pub uid: String,
    #[serde(rename = "sessionToken")]
    pub session_token: String,
    pub verified: bool,
}

#[derive(Deserialize)]
pub struct RecoveryEmailStatusResponse {
    pub email: String,
    pub verified: bool,
}

#[derive(Deserialize)]
pub struct AccountStatusResponse {
    pub exists: bool,
}

#[derive(Deserialize)]
pub struct OAuthTokenResponse {
    pub keys_jwe: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    pub scope: String,
    pub access_token: String,
}

#[derive(Deserialize)]
pub struct SignResponse {
    #[serde(rename = "cert")]
    pub certificate: String,
}

#[derive(Deserialize)]
pub struct KeysResponse {
    // ka: Vec<u8>,
    pub wrap_kb: Vec<u8>,
}

#[derive(Deserialize)]
pub struct ProfileResponse {
    pub uid: String,
    pub email: String,
    pub locale: String,
    #[serde(rename = "amrValues")]
    pub amr_values: Vec<String>,
    #[serde(rename = "twoFactorAuthentication")]
    pub two_factor_authentication: bool,
    pub avatar: String,
    #[serde(rename = "avatarDefault")]
    pub avatar_default: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use openssl::hash::MessageDigest;
    use openssl::pkcs5::pbkdf2_hmac;

    fn quick_strech_pwd(email: &str, pwd: &str) -> Vec<u8> {
        let salt = FxAClient::kwe("quickStretch", email);
        let digest = MessageDigest::sha256();
        let mut out = [0u8; 32];
        pbkdf2_hmac(pwd.as_bytes(), &salt, 1000, digest, &mut out);
        out.to_vec()
    }

    fn auth_pwd(email: &str, pwd: &str) -> String {
        let streched = quick_strech_pwd(email, pwd);
        let salt = [0u8; 0];
        let context = FxAClient::kw("authPW");
        let derived = FxAClient::derive_hkdf_sha256_key(&streched, &salt, &context, 32);
        hex::encode(derived)
    }

    #[test]
    fn test_quick_strech_pwd() {
        let email = "andré@example.org";
        let pwd = "pässwörd";
        let streched = hex::encode(quick_strech_pwd(email, pwd));
        assert_eq!(
            streched,
            "e4e8889bd8bd61ad6de6b95c059d56e7b50dacdaf62bd84644af7e2add84345d"
        );
    }

    #[test]
    fn test_auth_pwd() {
        let email = "andré@example.org";
        let pwd = "pässwörd";
        let auth_pwd = auth_pwd(email, pwd);
        assert_eq!(
            auth_pwd,
            "247b675ffb4c46310bc87e26d712153abe5e1c90ef00a4784594f97ef54f2375"
        );
    }

    #[test]
    fn live_account_test() {
        let email = "testfxarustclient@restmail.net";
        let pwd = "testfxarustclient@restmail.net";
        let auth_pwd = auth_pwd(email, pwd);

        let config = Config::stable().unwrap();
        let client = FxAClient::new(&config);

        let resp = client.login(&email, &auth_pwd, false).unwrap();
        println!("Session Token obtained: {}", &resp.session_token);
        let session_token = hex::decode(resp.session_token).unwrap();

        let resp = client
            .oauth_token_with_assertion("5882386c6d801776", &session_token, &["profile"])
            .unwrap();
        println!("OAuth Token obtained: {}", &resp.access_token);
    }
}

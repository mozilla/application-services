extern crate base64;
#[macro_use]
extern crate error_chain;
extern crate hawk;
extern crate hex;
extern crate hkdf;
extern crate hmac;
#[macro_use]
extern crate log;
extern crate openssl;
extern crate reqwest;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate sha2;
extern crate url;

use std::collections::HashMap;
use std::mem;

use self::login_sm::FxALoginState::*;
use self::login_sm::*;
use self::login_sm::{FxALoginState, FxALoginStateMachine};
use errors::*; // TODO: Error conflict because of the line bellow
use http_client::browser_id::{jwt_utils, BrowserIDKeyPair};
use http_client::FxAClient;

mod errors;
mod http_client;
mod login_sm;
mod util;

#[derive(Serialize, Deserialize)]
pub struct FxAConfig {
    // These URLs need a trailing slash if a path is specified!
    pub auth_url: String,
    pub oauth_url: String,
    pub profile_url: String,
}

impl FxAConfig {
    pub fn release() -> FxAConfig {
        FxAConfig {
            auth_url: "https://api.accounts.firefox.com/v1/".to_string(),
            oauth_url: "https://oauth.accounts.firefox.com/v1/".to_string(),
            profile_url: "https://oauth.accounts.firefox.com/v1/".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct FxAStateV1 {
    config: FxAConfig,
    uid: String,
    email: String,
    login_state: FxALoginState,
    oauth_tokens_cache: HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "schema_version")]
enum FxAState {
    V1(FxAStateV1),
}

#[derive(Deserialize)]
pub struct FxAWebChannelResponse {
    uid: String,
    email: String,
    verified: bool,
    #[serde(rename = "sessionToken")]
    session_token: String,
    #[serde(rename = "keyFetchToken")]
    key_fetch_token: String,
    #[serde(rename = "unwrapBKey")]
    unwrap_kb: String,
}

impl FxAWebChannelResponse {
    pub fn from_json(json: &str) -> Result<FxAWebChannelResponse> {
        Ok(serde_json::from_str(json)?)
    }
}

pub struct FirefoxAccount {
    state: FxAStateV1,
}

pub type SyncKeys = (String, String);

impl FirefoxAccount {
    // Initialize state from Firefox Accounts credentials obtained using the
    // web flow.
    pub fn from_credentials(
        config: FxAConfig,
        credentials: FxAWebChannelResponse,
    ) -> Result<FirefoxAccount> {
        let session_token = hex::decode(credentials.session_token)?;
        let key_fetch_token = hex::decode(credentials.key_fetch_token)?;
        let unwrap_kb = hex::decode(credentials.unwrap_kb)?;
        let login_state_data = ReadyForKeysState::new(session_token, key_fetch_token, unwrap_kb);
        let login_state = if credentials.verified {
            EngagedAfterVerified(login_state_data)
        } else {
            EngagedBeforeVerified(login_state_data)
        };

        Ok(FirefoxAccount {
            state: FxAStateV1 {
                config,
                uid: credentials.uid,
                email: credentials.email,
                login_state,
                oauth_tokens_cache: HashMap::new(),
            },
        })
    }

    pub fn from_json(data: &str) -> Result<FirefoxAccount> {
        let fxa_state: FxAState = serde_json::from_str(data)?;
        match fxa_state {
            FxAState::V1(state) => Ok(FirefoxAccount { state: state }),
        }
    }

    pub fn to_json(&self) -> Result<String> {
        let mut json = serde_json::to_value(&self.state)?;
        // Hack: Instead of reconstructing the FxAState enum (and moving self.state!),
        // we add the schema_version key manually.
        let obj = json.as_object_mut().expect("Not an object!");
        obj.insert("schema_version".to_string(), json!("V1"));
        Ok(json!(obj).to_string())
    }

    pub fn to_married(&mut self) -> Option<&MarriedState> {
        self.advance();
        match self.state.login_state {
            Married(ref married) => Some(married),
            _ => None,
        }
    }

    pub fn advance(&mut self) {
        let client = FxAClient::new(&self.state.config);
        let state_machine = FxALoginStateMachine::new(client);
        let state = mem::replace(&mut self.state.login_state, Separated);
        self.state.login_state = state_machine.advance(state);
    }

    pub fn get_oauth_token(&mut self, scopes: Vec<&str>) -> Result<String> {
        let scopes_key = self.scopes_key(&scopes);
        if let Some(cached_token) = self.state.oauth_tokens_cache.get(&scopes_key) {
            return Ok(cached_token.clone());
        }
        let client = FxAClient::new(&self.state.config);
        let session_token = match FirefoxAccount::session_token_from_state(&self.state.login_state)
        {
            Some(session_token) => session_token,
            None => bail!(ErrorKind::NoSessionToken),
        };
        let response = client.oauth_authorize(session_token, &scopes)?;
        let token = response.access_token;
        self.state
            .oauth_tokens_cache
            .insert(scopes_key, token.clone());
        Ok(token)
    }

    fn scopes_key(&self, scopes: &[&str]) -> String {
        scopes.join("|")
    }

    fn session_token_from_state(state: &FxALoginState) -> Option<&[u8]> {
        match state {
            &Separated => None,
            // Despite all these states implementing the same trait we can't treat
            // them in a single arm, so this will do for now :/
            &EngagedBeforeVerified(ref state) | &EngagedAfterVerified(ref state) => {
                Some(state.session_token())
            }
            &CohabitingBeforeKeyPair(ref state) => Some(state.session_token()),
            &CohabitingAfterKeyPair(ref state) => Some(state.session_token()),
            &Married(ref state) => Some(state.session_token()),
        }
    }

    pub fn generate_assertion(&mut self, audience: &str) -> Result<String> {
        let married = match self.to_married() {
            Some(married) => married,
            None => bail!(ErrorKind::NotMarried),
        };
        let private_key = married.key_pair().private_key();
        let certificate = married.certificate();
        Ok(jwt_utils::create_assertion(
            private_key,
            &certificate,
            audience,
        )?)
    }

    pub fn get_sync_keys(&mut self) -> Result<SyncKeys> {
        let married = match self.to_married() {
            Some(married) => married,
            None => bail!(ErrorKind::NotMarried),
        };
        let sync_key = hex::encode(married.sync_key());
        Ok((sync_key, married.xcs().to_string()))
    }

    pub fn handle_push_message() {
        panic!("Not implemented yet!")
    }

    pub fn get_profile() {
        panic!("Not implemented yet!")
    }

    pub fn register_device() {
        panic!("Not implemented yet!")
    }

    pub fn get_devices_list() {
        panic!("Not implemented yet!")
    }

    pub fn send_message() {
        panic!("Not implemented yet!")
    }

    pub fn retrieve_messages() {
        panic!("Not implemented yet!")
    }

    pub fn sign_out(mut self) {
        let client = FxAClient::new(&self.state.config);
        client.sign_out();
        self.state.login_state = FxALoginState::Separated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize() {
        let config = FxAConfig::release();
        let mut fxa1 = FirefoxAccount::from_credentials(
            config,
            FxAWebChannelResponse {
                uid: "123456".to_string(),
                email: "foo@bar.com".to_string(),
                verified: false,
                session_token: "12".to_string(),
                key_fetch_token: "34".to_string(),
                unwrap_kb: "56".to_string(),
            },
        ).unwrap();
        let fxa1_json = fxa1.to_json().unwrap();
        drop(fxa1);
        let fxa2 = FirefoxAccount::from_json(&fxa1_json).unwrap();
        let fxa2_json = fxa2.to_json().unwrap();
        assert_eq!(fxa1_json, fxa2_json);
    }
}

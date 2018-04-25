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

use errors::*; // TODO: Error conflict because of the line bellow
use fxa_client::FxAClient;
use self::login_sm::*;
use self::login_sm::{FxAState, FxALoginStateMachine};
use self::login_sm::FxAState::*;

mod errors;
mod fxa_client;
mod login_sm;
mod util;

#[derive(Serialize, Deserialize)]
pub struct FxAConfig {
  // These URLs need a trailing slash if a path is specified!
  auth_url: String,
  oauth_url: String,
  profile_url: String
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
  unwrap_kb: String
}

impl FxAWebChannelResponse {
  pub fn from_json(json: &str) -> Result<FxAWebChannelResponse> {
    serde_json::from_str(json).chain_err(|| "Could not parse JSON.")
  }
}

#[derive(Serialize, Deserialize)]
pub struct FirefoxAccount {
  config: FxAConfig,
  uid: String,
  email: String,
  state: FxAState,
  oauth_tokens_cache: HashMap<String, String>
}

impl FirefoxAccount {
  // Initialize state from Firefox Accounts credentials obtained using the
  // web flow.
  pub fn from_credentials(config: FxAConfig, credentials: FxAWebChannelResponse) -> Result<FirefoxAccount> {
    let session_token = hex::decode(credentials.session_token)
      .chain_err(|| "Could not decode session_token.")?;
    let key_fetch_token = hex::decode(credentials.key_fetch_token)
      .chain_err(|| "Could not decode key_fetch_token.")?;
    let unwrap_kb = hex::decode(credentials.unwrap_kb)
      .chain_err(|| "Could not decode unwrap_kb.")?;
    let state_data = ReadyForKeysState::new(session_token,
      key_fetch_token, unwrap_kb);
    let state = if credentials.verified {
      EngagedAfterVerified(state_data)
    } else {
      EngagedBeforeVerified(state_data)
    };

    Ok(FirefoxAccount {
      config: config,
      uid: credentials.uid,
      email: credentials.email,
      state: state,
      oauth_tokens_cache: HashMap::new()
    })
  }

  pub fn from_json(data: &str) -> Result<FirefoxAccount> {
    serde_json::from_str(data)
      .chain_err(|| "Could not read from JSON representation.")
  }

  pub fn to_json(&self) -> Result<String> {
    serde_json::to_string(self)
      .chain_err(|| "Could not create JSON representation.")
  }

  pub fn advance(mut self) -> FxAState {
    let client = FxAClient::new(&self.config);
    let state_machine = FxALoginStateMachine::new(client);
    self.state = state_machine.advance(self.state);
    self.state
  }

  pub fn get_oauth_token(&mut self, scope: &str) -> Result<String> {
    if let Some(cached_token) = self.oauth_tokens_cache.get(scope) {
      return Ok(cached_token.clone());
    }
    let client = FxAClient::new(&self.config);
    let session_token = match FirefoxAccount::session_token_from_state(&self.state) {
      Some(session_token) => session_token,
      None => bail!("Not in a session token state!")
    };
    let response = client.oauth_authorize(session_token, scope)?;
    let token = response.access_token;
    self.oauth_tokens_cache.insert(scope.to_string(), token.clone());
    Ok(token)
  }

  fn session_token_from_state(state: &FxAState) -> Option<&[u8]> {
    match state {
      &Separated => None,
      // Despite all these states implementing the same trait we can't treat
      // them in a single arm, so this will do for now :/
      &EngagedBeforeVerified(ref state) => Some(state.session_token()),
      &EngagedAfterVerified(ref state) => Some(state.session_token()),
      &CohabitingBeforeKeyPair(ref state) => Some(state.session_token()),
      &CohabitingAfterKeyPair(ref state) => Some(state.session_token()),
      &Married (ref state) => Some(state.session_token())
    }
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
    let client = FxAClient::new(&self.config);
    client.sign_out();
    self.state = FxAState::Separated
  }
}

extern crate base64;
#[macro_use]
extern crate error_chain;
extern crate hawk;
extern crate hex;
extern crate hkdf;
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

use error::*;
use fxa_client::*;

mod error;
mod fxa_client;

#[derive(Serialize, Deserialize)]
pub struct FxAConfig {
  // These URLs need a trailing slash if a path is specified!
  auth_url: String,
  oauth_url: String,
  profile_url: String
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
  pub fn from_credentials(config: FxAConfig, data: &str) -> Result<FirefoxAccount> {
    let credentials: FxALoginResponse = serde_json::from_str(data)
      .chain_err(|| "Could not deserialize login response.")?;

    let state = if credentials.verified {
      FxAState::SignedIn(credentials.session_token)
    } else {
      FxAState::Unverified(credentials.session_token)
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
    let state_machine = FxALoginStateMachine {client: client};
    // TODO: Passing the UID is a code-smell.
    self.state = state_machine.advance(self.state, &self.uid);
    self.state
  }

  pub fn get_oauth_token(&mut self, scope: &str) -> Result<String> {
    if let Some(cached_token) = self.oauth_tokens_cache.get(scope) {
      return Ok(cached_token.clone());
    }
    let client = FxAClient::new(&self.config);
    let session_token = match &self.state {
      &FxAState::SignedIn(ref session_token) |
      &FxAState::Unverified(ref session_token) => session_token,
      _ => { bail!("Not a session token state: {:?}.", self.state) }
    };
    let response = client.oauth_authorize(session_token, scope)?;
    let token = response.access_token;
    self.oauth_tokens_cache.insert(scope.to_string(), token.clone());
    Ok(token)
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
    self.state = FxAState::SignedOut
  }
}

struct FxALoginStateMachine<'a> {
  client: FxAClient<'a>
}

impl<'a> FxALoginStateMachine<'a> {
  fn advance(&self, from: FxAState, uid: &String) -> FxAState {
    let mut cur_state = from;
    loop {
      let cur_state_discriminant = std::mem::discriminant(&cur_state);
      let new_state = self.advance_one(cur_state, uid);
      let new_state_discriminant = std::mem::discriminant(&new_state);
      cur_state = new_state;
      if cur_state_discriminant == new_state_discriminant { break }
    }
    cur_state
  }

  fn advance_one(&self, from: FxAState, uid: &String) -> FxAState {
    let same = from.clone();
    match from {
      FxAState::Unverified(session_token) => {
        match self.client.recovery_email_status(&session_token) {
          // TODO: Add logging in error cases!
          Ok(RecoveryEmailStatusResponse { verified: false, .. }) => same,
          Ok(RecoveryEmailStatusResponse { verified: true, .. }) => FxAState::SignedIn(session_token),
          // TODO: this recovery mechanism is cool... but doesn't apply everywhere we make a request
          Err(Error(ErrorKind::RemoteError(401, ..), ..)) => {
            match self.client.account_status(uid) {
              Ok(AccountStatusResponse { exists: true }) => FxAState::LoginFailed,
              Ok(AccountStatusResponse { exists: false }) => FxAState::SignedOut,
              Err(_) => same
            }
          },
          Err(_) => same
        }
      },
      FxAState::SignedIn(_) | FxAState::LoginFailed | FxAState::SignedOut => same
    }
  }
}

#[derive(Deserialize)]
struct FxALoginResponse {
  uid: String,
  email: String,
  verified: bool,
  #[serde(rename = "sessionToken")]
  session_token: String
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum FxAState {
  Unverified(String), // Session Token
  SignedIn(String), // Session Token
  LoginFailed,
  SignedOut
}

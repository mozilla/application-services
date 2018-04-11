extern crate hawk;
extern crate hkdf;
extern crate reqwest;
extern crate serde_json;
extern crate sha2;
extern crate url;

#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate serde_derive;

mod error;
pub use error::*;

mod crypto;
mod hawk_request;
mod util;

mod fxa_client;
use fxa_client::FxAClient;

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
  state: FxAState
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
      state: state
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
    self.state = state_machine.advance(self.state);
    self.state
  }

  pub fn get_oauth_token(&self, scope: &str) -> Result<String> {
    bail!("Not implemented yet!")
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
  fn advance(&self, from: FxAState) -> FxAState {
    let mut cur_state = from;
    loop {
      let cur_state_discriminant = std::mem::discriminant(&cur_state);
      let new_state = self.advance_one(cur_state);
      let new_state_discriminant = std::mem::discriminant(&new_state);
      cur_state = new_state;
      if cur_state_discriminant == new_state_discriminant { break }
    }
    cur_state
  }

  fn advance_one(&self, from: FxAState) -> FxAState {
    match from {
      FxAState::Unverified(session_token) => {
        // TODO: Can either go to signin (after /acount/status) or loginfailed
        FxAState::SignedIn(session_token)
      },
      FxAState::SignedIn(_) => from, // TODO: We should squeeze a quick /account/status check here.
      FxAState::LoginFailed => from, // Not much we can do here, waiting for the user to re-enter credentials.
      FxAState::SignedOut => FxAState::SignedOut
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

#[derive(Serialize, Deserialize)]
pub enum FxAState {
  Unverified(String), // Session Token
  SignedIn(String), // Session Token
  LoginFailed,
  SignedOut
}

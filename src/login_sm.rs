use std;
use std::time::{SystemTime, UNIX_EPOCH};

use errors::*;
use fxa_client::*;
use fxa_client::browser_id::BrowserIDKeyPair;
use fxa_client::browser_id::rsa::RSABrowserIDKeyPair;
use fxa_client::errors::Error as FxAClientError;
use fxa_client::errors::ErrorKind::RemoteError as FxAClientRemoteError;
use login_sm::FxAState::*;

pub struct FxALoginStateMachine<'a> {
  client: FxAClient<'a>
}

impl<'a> FxALoginStateMachine<'a> {
  pub fn new(client: FxAClient<'a>) -> FxALoginStateMachine {
    FxALoginStateMachine {
      client
    }
  }

  pub fn advance(&self, from: FxAState) -> FxAState {
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

  // Returns None if the state hasn't changed.
  fn advance_one(&self, from: FxAState) -> FxAState {
    match from {
      Married(state) => {
        let now = now();
        if state.token_keys_and_key_pair.key_pair_expires_at > now {
          CohabitingBeforeKeyPair(state.token_keys_and_key_pair.token_and_keys)
        } else if state.certificate_expires_at > now {
          CohabitingAfterKeyPair(state.token_keys_and_key_pair)
        } else {
          Married(state) // same
        }
      },
      CohabitingBeforeKeyPair(state) => {
        let key_pair = match FxAClient::key_pair(2048) {
          Ok(key_pair) => key_pair,
          Err(_) => { return Separated }
        };
        let new_state = CohabitingAfterKeyPairState {
          token_and_keys: state,
          key_pair,
          key_pair_expires_at: now() + 30 * 24 * 3600 * 1000
        };
        CohabitingAfterKeyPair(new_state)
      },
      CohabitingAfterKeyPair(state) => {
        let resp = self.client.sign(&state.token_and_keys.session_token, (&state.key_pair).public_key());
        match resp {
          Ok(resp) => {
            let new_state = MarriedState {
              token_keys_and_key_pair: state,
              certificate: resp.certificate,
              certificate_expires_at: now() + 24 * 3600 * 1000
            };
            Married(new_state)
          },
          Err(FxAClientError(FxAClientRemoteError(..), ..)) => Separated,
          Err(_) => CohabitingAfterKeyPair(state) // same
        }
      },
      EngagedBeforeVerified(state) => {
        self.handle_ready_for_key_state(EngagedBeforeVerified, state)
      },
      EngagedAfterVerified(state) => {
        self.handle_ready_for_key_state(EngagedAfterVerified, state)
      },
      Separated => from
    }
  }

  fn handle_ready_for_key_state<F: FnOnce(ReadyForKeysState)->FxAState>(&self, same: F, state: ReadyForKeysState) -> FxAState {
    let resp = self.client.keys(&state.key_fetch_token);
    match resp {
      Ok(resp) => {
        let kb = match resp.wrap_kb.xored_with(&state.unwrap_kb) {
          Ok(kb) => kb,
          Err(_) => { return same(state) }
        };
        let sync_key = FxAClient::derive_sync_key(&kb);
        let xcs = FxAClient::compute_client_state(&kb);
        CohabitingBeforeKeyPair(TokenAndKeysState {
          session_token: state.session_token.to_vec(),
          sync_key,
          xcs
        })
      },
      Err(FxAClientError(FxAClientRemoteError(_, 104, ..), ..)) => same(state), // Response: Unverified
      Err(FxAClientError(FxAClientRemoteError(..), ..)) => Separated,
      Err(_) => same(state)
    }
  }
}

// Gets the unix epoch in ms.
// TODO: Probably doesn't belong here.
fn now() -> u64 {
  let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH)
    .expect("Something is very wrong.");
  since_epoch.as_secs() * 1000 + since_epoch.subsec_nanos() as u64 / 1_000_000
}

trait Xorable {
  fn xored_with(&self, other: &[u8]) -> Result<Vec<u8>>;
}

impl Xorable for [u8] {
  fn xored_with(&self, other: &[u8]) -> Result<Vec<u8>> {
    if self.len() != other.len() {
      bail!("Slices have different sizes.")
    }
    Ok(self.iter().zip(other.iter()).map(|(&x, &y)| x ^ y).collect())
  }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum FxAState {
  Married(MarriedState),
  CohabitingBeforeKeyPair(CohabitingBeforeKeyPairState),
  CohabitingAfterKeyPair(CohabitingAfterKeyPairState),
  EngagedBeforeVerified(EngagedBeforeVerifiedState),
  EngagedAfterVerified(EngagedAfterVerifiedState),
  Separated
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MarriedState {
  token_keys_and_key_pair: TokenKeysAndKeyPairState,
  certificate: String,
  certificate_expires_at: u64
}

pub type CohabitingBeforeKeyPairState = TokenAndKeysState;
pub type CohabitingAfterKeyPairState = TokenKeysAndKeyPairState;
pub type EngagedBeforeVerifiedState = ReadyForKeysState;
pub type EngagedAfterVerifiedState = ReadyForKeysState;

#[derive(Serialize, Deserialize, Debug)]
pub struct ReadyForKeysState {
  session_token: Vec<u8>,
  key_fetch_token: Vec<u8>,
  unwrap_kb: Vec<u8>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TokenAndKeysState {
  session_token: Vec<u8>,
  sync_key: Vec<u8>,
  xcs: String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TokenKeysAndKeyPairState {
  token_and_keys: TokenAndKeysState,
  key_pair: RSABrowserIDKeyPair,
  key_pair_expires_at: u64
}

impl ReadyForKeysState {
  pub fn new(session_token: Vec<u8>, key_fetch_token: Vec<u8>, unwrap_kb: Vec<u8>) -> ReadyForKeysState {
    ReadyForKeysState {
      session_token,
      key_fetch_token,
      unwrap_kb
    }
  }
}

pub trait SessionTokenState {
  fn session_token(&self) -> &[u8];
}

impl SessionTokenState for ReadyForKeysState {
  fn session_token(&self) -> &[u8] {
    &self.session_token
  }
}

impl SessionTokenState for TokenAndKeysState {
  fn session_token(&self) -> &[u8] {
    &self.session_token
  }
}

impl SessionTokenState for TokenKeysAndKeyPairState {
  fn session_token(&self) -> &[u8] {
    self.token_and_keys.session_token()
  }
}

impl SessionTokenState for MarriedState {
  fn session_token(&self) -> &[u8] {
    self.token_keys_and_key_pair.session_token()
  }
}

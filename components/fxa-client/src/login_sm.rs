/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    errors::*,
    http_client::{self, browser_id::rsa::RSABrowserIDKeyPair, *},
    util::{now, Xorable},
    Config,
};
use serde_derive::*;
use std::sync::Arc;

pub struct LoginStateMachine<'a> {
    config: &'a Config,
    client: Arc<dyn http_client::browser_id::FxABrowserIDClient>,
}

impl<'a> LoginStateMachine<'a> {
    pub fn new(
        config: &'a Config,
        client: Arc<dyn http_client::browser_id::FxABrowserIDClient>,
    ) -> LoginStateMachine<'_> {
        LoginStateMachine { config, client }
    }

    pub fn advance(&self, from: LoginState) -> LoginState {
        let mut cur_state = from;
        loop {
            let cur_state_discriminant = std::mem::discriminant(&cur_state);
            let new_state = self.advance_one(cur_state);
            let new_state_discriminant = std::mem::discriminant(&new_state);
            cur_state = new_state;
            if cur_state_discriminant == new_state_discriminant {
                break;
            }
        }
        cur_state
    }

    fn advance_one(&self, from: LoginState) -> LoginState {
        log::info!("advancing from state {:?}", from);
        match from {
            LoginState::Married(state) => {
                let now = now();
                log::debug!("Checking key pair and certificate freshness.");
                if now > state.token_keys_and_key_pair.key_pair_expires_at {
                    log::info!("Key pair has expired. Transitioning to CohabitingBeforeKeyPair.");
                    LoginState::CohabitingBeforeKeyPair(
                        state.token_keys_and_key_pair.token_and_keys,
                    )
                } else if now > state.certificate_expires_at {
                    log::info!("Certificate has expired. Transitioning to CohabitingAfterKeyPair.");
                    LoginState::CohabitingAfterKeyPair(state.token_keys_and_key_pair)
                } else {
                    log::info!("Key pair and certificate are fresh; staying Married.");
                    LoginState::Married(state) // same
                }
            }
            LoginState::CohabitingBeforeKeyPair(state) => {
                log::debug!("Generating key pair.");
                let key_pair = match browser_id::key_pair(2048) {
                    Ok(key_pair) => key_pair,
                    Err(_) => {
                        log::error!("Failed to generate key pair! Transitioning to Separated.");
                        return LoginState::Separated(state.base);
                    }
                };
                log::info!("Key pair generated! Transitioning to CohabitingAfterKeyPairState.");
                let new_state = CohabitingAfterKeyPairState {
                    token_and_keys: state,
                    key_pair,
                    key_pair_expires_at: now() + 30 * 24 * 3600 * 1000,
                };
                LoginState::CohabitingAfterKeyPair(new_state)
            }
            LoginState::CohabitingAfterKeyPair(state) => {
                log::debug!("Signing public key.");
                let resp = self.client.sign(
                    &self.config,
                    &state.token_and_keys.session_token,
                    &state.key_pair,
                );
                match resp {
                    Ok(resp) => {
                        log::info!("Signed public key! Transitioning to Married.");
                        let new_state = MarriedState {
                            token_keys_and_key_pair: state,
                            certificate: resp.certificate,
                            certificate_expires_at: now() + 24 * 3600 * 1000,
                        };
                        LoginState::Married(new_state)
                    }
                    Err(e) => {
                        if let ErrorKind::RemoteError { .. } = e.kind() {
                            log::error!("Server error: {:?}. Transitioning to Separated.", e);
                            LoginState::Separated(state.token_and_keys.base)
                        } else {
                            log::error!(
                                "Unknown error: ({:?}). Assuming transient, not transitioning.",
                                e
                            );
                            LoginState::CohabitingAfterKeyPair(state)
                        }
                    }
                }
            }
            LoginState::EngagedBeforeVerified(state) => {
                self.handle_ready_for_key_state(LoginState::EngagedBeforeVerified, state)
            }
            LoginState::EngagedAfterVerified(state) => {
                self.handle_ready_for_key_state(LoginState::EngagedAfterVerified, state)
            }
            LoginState::Separated(_) => from,
            LoginState::Unknown => from,
        }
    }

    fn handle_ready_for_key_state<F: FnOnce(ReadyForKeysState) -> LoginState>(
        &self,
        same: F,
        state: ReadyForKeysState,
    ) -> LoginState {
        log::debug!("Fetching keys.");
        let resp = self.client.keys(&self.config, &state.key_fetch_token);
        match resp {
            Ok(resp) => {
                let kb = match resp.wrap_kb.xored_with(&state.unwrap_kb) {
                    Ok(kb) => kb,
                    Err(_) => {
                        log::error!("Failed to unwrap keys response!  Transitioning to Separated.");
                        return same(state);
                    }
                };
                log::info!("Unwrapped keys response.  Transition to CohabitingBeforeKeyPair.");
                let sync_key = browser_id::derive_sync_key(&kb);
                let xcs = browser_id::compute_client_state(&kb);
                LoginState::CohabitingBeforeKeyPair(TokenAndKeysState {
                    base: state.base,
                    session_token: state.session_token.to_vec(),
                    sync_key,
                    xcs,
                })
            }
            Err(e) => match e.kind() {
                ErrorKind::RemoteError { errno: 104, .. } => {
                    log::warn!("Account not yet verified, not transitioning.");
                    same(state)
                }
                ErrorKind::RemoteError { .. } => {
                    log::error!("Server error: {:?}. Transitioning to Separated.", e);
                    LoginState::Separated(state.base)
                }
                _ => {
                    log::error!(
                        "Unknown error: ({:?}). Assuming transient, not transitioning.",
                        e
                    );
                    same(state)
                }
            },
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum LoginState {
    Married(MarriedState),
    CohabitingBeforeKeyPair(CohabitingBeforeKeyPairState),
    CohabitingAfterKeyPair(CohabitingAfterKeyPairState),
    EngagedBeforeVerified(EngagedBeforeVerifiedState),
    EngagedAfterVerified(EngagedAfterVerifiedState),
    Separated(SeparatedState),
    Unknown, // If a client never uses the session_token flows, we will be in this state.
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarriedState {
    token_keys_and_key_pair: TokenKeysAndKeyPairState,
    certificate: String,
    certificate_expires_at: u64,
}

pub type CohabitingBeforeKeyPairState = TokenAndKeysState;
pub type CohabitingAfterKeyPairState = TokenKeysAndKeyPairState;
pub type EngagedBeforeVerifiedState = ReadyForKeysState;
pub type EngagedAfterVerifiedState = ReadyForKeysState;
pub type SeparatedState = BaseState;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReadyForKeysState {
    base: BaseState,
    session_token: Vec<u8>,
    key_fetch_token: Vec<u8>,
    unwrap_kb: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TokenAndKeysState {
    base: BaseState,
    session_token: Vec<u8>,
    sync_key: Vec<u8>,
    xcs: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TokenKeysAndKeyPairState {
    token_and_keys: TokenAndKeysState,
    key_pair: RSABrowserIDKeyPair,
    key_pair_expires_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BaseState {
    uid: String,
    email: String,
}

impl ReadyForKeysState {
    pub fn new(
        uid: String,
        email: String,
        session_token: Vec<u8>,
        key_fetch_token: Vec<u8>,
        unwrap_kb: Vec<u8>,
    ) -> ReadyForKeysState {
        ReadyForKeysState {
            base: BaseState { uid, email },
            session_token,
            key_fetch_token,
            unwrap_kb,
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

impl MarriedState {
    pub fn key_pair(&self) -> &RSABrowserIDKeyPair {
        &self.token_keys_and_key_pair.key_pair
    }
    pub fn certificate(&self) -> &str {
        &self.certificate
    }
    pub fn sync_key(&self) -> &[u8] {
        &self.token_keys_and_key_pair.token_and_keys.sync_key
    }
    pub fn xcs(&self) -> &str {
        &self.token_keys_and_key_pair.token_and_keys.xcs
    }
}

impl LoginState {
    pub fn into_separated(self) -> Self {
        use self::LoginState::*;
        match self {
            Married(state) => Separated(state.token_keys_and_key_pair.token_and_keys.base),
            CohabitingBeforeKeyPair(state) => Separated(state.base),
            CohabitingAfterKeyPair(state) => Separated(state.token_and_keys.base),
            EngagedBeforeVerified(state) => Separated(state.base),
            EngagedAfterVerified(state) => Separated(state.base),
            Separated(state) => Separated(state),
            Unknown => panic!("Insane state."),
        }
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    errors::*,
    http_client::browser_id::jwt_utils,
    login_sm::{LoginState, LoginStateMachine, MarriedState, ReadyForKeysState, SessionTokenState},
    Config, FirefoxAccount, StateV2,
};
use serde_derive::*;
use std::collections::HashMap;

impl FirefoxAccount {
    // Initialize state from Firefox Accounts credentials obtained using the
    // web flow.
    pub fn from_credentials(
        content_url: &str,
        client_id: &str,
        redirect_uri: &str,
        credentials: WebChannelResponse,
    ) -> Result<Self> {
        let config = Config::new(content_url, client_id, redirect_uri);
        let session_token = hex::decode(credentials.session_token)?;
        let key_fetch_token = hex::decode(credentials.key_fetch_token)?;
        let unwrap_kb = hex::decode(credentials.unwrap_kb)?;
        let login_state_data = ReadyForKeysState::new(
            credentials.uid,
            credentials.email,
            session_token,
            key_fetch_token,
            unwrap_kb,
        );
        let login_state = if credentials.verified {
            LoginState::EngagedAfterVerified(login_state_data)
        } else {
            LoginState::EngagedBeforeVerified(login_state_data)
        };

        Ok(Self::from_state(StateV2 {
            config,
            login_state,
            refresh_token: None,
            scoped_keys: HashMap::new(),
            session_token: None,
        }))
    }

    fn advance_to_married(&mut self) -> Option<&MarriedState> {
        self.advance();
        match self.state.login_state {
            LoginState::Married(ref married) => Some(married),
            _ => None,
        }
    }

    pub fn advance(&mut self) {
        let state_machine = LoginStateMachine::new(&self.state.config, self.client.clone());
        let state = std::mem::replace(&mut self.state.login_state, LoginState::Unknown);
        self.state.login_state = state_machine.advance(state);
    }

    pub(crate) fn session_token_from_state(state: &LoginState) -> Option<&[u8]> {
        match state {
            &LoginState::Separated(_) | LoginState::Unknown => None,
            // Despite all these states implementing the same trait we can't treat
            // them in a single arm, so this will do for now :/
            &LoginState::EngagedBeforeVerified(ref state)
            | &LoginState::EngagedAfterVerified(ref state) => Some(state.session_token()),
            &LoginState::CohabitingBeforeKeyPair(ref state) => Some(state.session_token()),
            &LoginState::CohabitingAfterKeyPair(ref state) => Some(state.session_token()),
            &LoginState::Married(ref state) => Some(state.session_token()),
        }
    }

    pub fn generate_assertion(&mut self, audience: &str) -> Result<String> {
        let married = match self.advance_to_married() {
            Some(married) => married,
            None => return Err(ErrorKind::NotMarried.into()),
        };
        let key_pair = married.key_pair();
        let certificate = married.certificate();
        Ok(jwt_utils::create_assertion(
            key_pair,
            &certificate,
            audience,
        )?)
    }

    pub fn get_sync_keys(&mut self) -> Result<SyncKeys> {
        let married = match self.advance_to_married() {
            Some(married) => married,
            None => return Err(ErrorKind::NotMarried.into()),
        };
        let sync_key = hex::encode(married.sync_key());
        Ok(SyncKeys(sync_key, married.xcs().to_string()))
    }

    pub fn sign_out(mut self) {
        self.client.sign_out();
        self.state.login_state = self.state.login_state.into_separated();
    }
}

#[derive(Deserialize)]
pub struct WebChannelResponse {
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

impl WebChannelResponse {
    pub fn from_json(json: &str) -> Result<WebChannelResponse> {
        serde_json::from_str(json).map_err(Into::into)
    }
}

pub struct SyncKeys(pub String, pub String);

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

mod client_states;
mod extension;
mod key_schedule;
mod messages;
mod record_layer;
mod server_states;
mod tls_connection;
mod utils;
use crate::oauth::{AuthorizationPKCEParams, AuthorizationParameters};
use crate::FirefoxAccount;
use anyhow::Result;
use rc_crypto::rand;
use serde_derive::*;
use serde_json::json;
use std::cell::RefCell;
use tls_connection::{Connection, ServerConnection};
use tungstenite::{client::connect, Message};

#[derive(Deserialize)]
struct FirstMessage {
    channelid: String,
}
use std::sync::Mutex;
#[derive(Deserialize)]
struct OtherMessages {
    message: String,
}

#[derive(Deserialize, Clone)]
pub struct AuthMessage {
    pub client_id: String,
    pub scope: String,
    pub state: String,
    pub access_type: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub keys_jwk: String,
}

impl From<AuthMessage> for AuthorizationParameters {
    fn from(auth_message: AuthMessage) -> Self {
        Self {
            client_id: auth_message.client_id,
            scope: auth_message
                .scope
                .split_whitespace()
                .map(|s| s.to_string())
                .collect(),
            access_type: auth_message.access_type,
            state: auth_message.state,
            pkce_params: Some(AuthorizationPKCEParams {
                code_challenge: auth_message.code_challenge,
                code_challenge_method: auth_message.code_challenge_method,
            }),
            keys_jwk: Some(auth_message.keys_jwk),
        }
    }
}

/// This is a hacked together function to be used by the demo
/// Ideally, this behaviour should either be done across the FFI
/// Or figure out a more straight forward approach to websockets
/// that does not require a library that pulls in OpenSSL.
pub fn run_server(auth_fxa: &Mutex<FirefoxAccount>, channel_server_url: &str) -> Result<()> {
    let (socket, _) = connect(channel_server_url)?;
    let socket = RefCell::new(socket);
    let mut channel_key = vec![0u8; 32];
    rand::fill(&mut channel_key)?;
    let first_message = socket.borrow_mut().read_message().unwrap();
    let first_message: FirstMessage = serde_json::from_str(&first_message.into_text().unwrap())?;
    qr2term::print_qr(format!(
        "https://stable.dev.lcip.org/pair/#channel_id={}&channel_key={}",
        first_message.channelid,
        base64::encode_config(&channel_key, base64::URL_SAFE_NO_PAD)
    ))?;
    let psk_id = first_message.channelid.as_bytes();
    let mut server = ServerConnection::new(channel_key, psk_id.to_vec(), |data| {
        let encoded = base64::encode_config(data, base64::URL_SAFE_NO_PAD);
        socket.borrow_mut().write_message(Message::from(encoded))?;
        Ok(())
    })?;
    loop {
        let message = socket.borrow_mut().read_message()?;
        match message {
            msg @ Message::Text(_) | msg @ Message::Binary(_) => {
                let envelope: OtherMessages = serde_json::from_str(&msg.to_string())?;
                let message_bytes =
                    base64::decode_config(envelope.message, base64::URL_SAFE_NO_PAD)?;
                let res = server.recv(&message_bytes).unwrap();
                let inner_str = std::str::from_utf8(&res);
                if let Ok(s) = inner_str {
                    if s.is_empty() {
                        continue;
                    }
                    let json_val: serde_json::Value = serde_json::from_str(s)?;
                    if json_val.get("message").unwrap().as_str().unwrap() == "pair:supp:request" {
                        let body_metadata = json!({
                            "message": "pair:auth:metadata",
                            "data": {
                                "email": "tarikeshaq@gmail.com",
                                "avatar": "Nope",
                                "displayName": "Tarik's Awesome Demo",
                                "deviceName": "Tarik's Awesome Demo"
                            }
                        });
                        let resp = serde_json::to_string(&body_metadata).unwrap();
                        server.send(resp.as_bytes())?;
                        // Now to the actual data:
                        let json_data = json_val.get("data").unwrap();
                        let auth_message: AuthMessage =
                            serde_json::from_value(json_data.clone()).unwrap();
                        let code = auth_fxa
                            .lock()
                            .unwrap()
                            .authorize_code_using_session_token(auth_message.clone().into())?;
                        let resp = json!({
                            "data": {
                                "code": code,
                                "state": auth_message.state,
                                "redirect": format!("https://stable.dev.lcip.org/oauth/success/{}?code={}&state={}", auth_message.client_id, code, auth_message.state),
                            },
                            "message": "pair:auth:authorize"
                        });
                        let resp = serde_json::to_string(&resp).unwrap();
                        server.send(resp.as_bytes())?;
                    } else if json_val.get("message").unwrap().as_str().unwrap()
                        == "pair:supp:authorize"
                    {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

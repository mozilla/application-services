/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::CommandsHandler;
use ece::{
    Aes128GcmEceWebPush, LocalKeyPair, OpenSSLLocalKeyPair, OpenSSLRemotePublicKey, WebPushParams,
};
use errors::*;
use hex;
use ring::rand::{SecureRandom, SystemRandom};
use std::panic::RefUnwindSafe;
use sync::KeyBundle;
use ClientInstance;

const COMMAND_SEND_TAB: &'static str = "https://identity.mozilla.com/cmd/open-uri";

pub struct SendTab {
    ksync: Vec<u8>,
    kxcs: Vec<u8>,
    tab_received_callback: TabReceivedCallback,
}

#[derive(Serialize, Deserialize)]
pub struct EncryptedSendTabPayload {
    /// URL Safe Base 64 encrypted send-tab payload.
    encrypted: String,
}

impl EncryptedSendTabPayload {
    fn decrypt(self, keys: &SendTabKeysInternal) -> Result<SendTabPayload> {
        let encrypted = base64::decode_config(&self.encrypted, base64::URL_SAFE_NO_PAD).unwrap();
        let private_key = OpenSSLLocalKeyPair::new(&keys.private_key).unwrap();
        let decrypted =
            Aes128GcmEceWebPush::decrypt(&private_key, &keys.auth_secret, &encrypted).unwrap();
        Ok(serde_json::from_slice(&decrypted).unwrap())
    }
}

#[derive(Serialize, Deserialize)]
pub struct SendTabPayload {
    entries: Vec<TabData>,
}

impl SendTabPayload {
    pub fn single_tab(title: &str, url: &str) -> Self {
        SendTabPayload {
            entries: vec![TabData {
                title: title.to_string(),
                url: url.to_string(),
            }],
        }
    }
    fn encrypt(&self, keys: SendTabKeysPublic) -> Result<EncryptedSendTabPayload> {
        // TODO: unwraps
        let bytes = serde_json::to_vec(&self)?;
        let public_key = base64::decode_config(&keys.public_key, base64::URL_SAFE_NO_PAD).unwrap();
        let public_key = OpenSSLRemotePublicKey::from_raw(&public_key);
        let auth_secret =
            base64::decode_config(&keys.auth_secret, base64::URL_SAFE_NO_PAD).unwrap();
        let encrypted = Aes128GcmEceWebPush::encrypt(
            &public_key,
            &auth_secret,
            &bytes,
            WebPushParams::default(),
        )
        .unwrap();
        let encrypted = base64::encode_config(&encrypted, base64::URL_SAFE_NO_PAD);
        Ok(EncryptedSendTabPayload { encrypted })
    }
}

#[derive(Serialize, Deserialize)]
pub struct TabData {
    title: String,
    url: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct SendTabKeysInternal {
    public_key: Vec<u8>,
    private_key: Vec<u8>,
    auth_secret: Vec<u8>,
}

impl SendTabKeysInternal {
    fn from_random() -> Self {
        let key_pair = OpenSSLLocalKeyPair::generate_random().unwrap(); // TODO: unwrap :(
        let private_key = key_pair.to_raw();
        let public_key = key_pair.pub_as_raw().unwrap();
        let mut auth_secret = vec![0u8; 16];
        SystemRandom::new().fill(&mut auth_secret).unwrap();
        Self {
            public_key,
            private_key,
            auth_secret,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SendTabKeysPayload {
    /// Hex encoded kid (kXCS).
    kid: String,
    /// Base 64 encoded IV.
    #[serde(rename = "IV")]
    iv: String,
    /// Hex encoded hmac.
    hmac: String,
    /// Base 64 encoded ciphertext.
    ciphertext: String,
}

impl SendTabKeysPayload {
    fn decrypt(self, ksync: &[u8], kxcs: &[u8]) -> Result<SendTabKeysPublic> {
        // Most of the code here is copied from `EncryptedBso::decrypt`:
        // we can't use that method as-it because `EncryptedBso` forces
        // a payload id to be specified, which in turns make the Firefox
        // Desktop commands implementation angry.
        if hex::decode(self.kid)? != kxcs {
            return Err(ErrorKind::MismatchedKeys.into());
        }
        let key = KeyBundle::from_ksync_bytes(ksync)
            .map_err(|_| ErrorKind::SyncError("Error importing ksync"))?;
        if !key.verify_hmac_string(&self.hmac, &self.ciphertext).unwrap() {
            return Err(ErrorKind::HmacMismatch.into());
        }
        let iv = base64::decode(&self.iv)?;
        let ciphertext = base64::decode(&self.ciphertext)?;
        let cleartext = key.decrypt(&ciphertext, &iv).unwrap();
        Ok(serde_json::from_str(&cleartext)?)
    }
}

#[derive(Serialize, Deserialize)]
struct SendTabKeysPublic {
    /// URL Safe Base 64 encoded push public key.
    #[serde(rename = "publicKey")]
    public_key: String,
    /// URL Safe Base 64 encoded auth secret.
    #[serde(rename = "authSecret")]
    auth_secret: String,
}

impl SendTabKeysPublic {
    fn encrypt(self, ksync: &[u8], kxcs: &[u8]) -> Result<SendTabKeysPayload> {
        // Most of the code here is copied from `CleartextBso::encrypt`:
        // we can't use that method as-it because `CleartextBso` forces
        // a payload id to be specified, which in turns make the Firefox
        // Desktop commands implementation angry.
        let key = KeyBundle::from_ksync_bytes(ksync)
            .map_err(|_| ErrorKind::SyncError("Error importing ksync"))?;
        let cleartext = serde_json::to_vec(&self)?;
        let (enc_bytes, iv) = key.encrypt_bytes_rand_iv(&cleartext).unwrap(); // TODO: unwrap :/
        let iv_base64 = base64::encode(&iv);
        let enc_base64 = base64::encode(&enc_bytes);
        let hmac = key.hmac_string(enc_base64.as_bytes()).unwrap();
        Ok(SendTabKeysPayload {
            kid: hex::encode(kxcs),
            iv: iv_base64,
            hmac,
            ciphertext: enc_base64,
        })
    }
}

impl From<SendTabKeysInternal> for SendTabKeysPublic {
    fn from(internal: SendTabKeysInternal) -> Self {
        Self {
            public_key: base64::encode_config(&internal.public_key, base64::URL_SAFE_NO_PAD),
            auth_secret: base64::encode_config(&internal.auth_secret, base64::URL_SAFE_NO_PAD),
        }
    }
}

impl SendTab {
    pub fn new(ksync: &[u8], kxcs: &[u8], tab_received_callback: TabReceivedCallback) -> Self {
        Self {
            ksync: ksync.to_vec(),
            kxcs: kxcs.to_vec(),
            tab_received_callback,
        }
    }

    pub fn build_send_command(
        &self,
        target: &ClientInstance,
        send_tab_payload: &SendTabPayload,
    ) -> Result<serde_json::Value> {
        let command = target
            .available_commands
            .get(COMMAND_SEND_TAB)
            .ok_or_else(|| ErrorKind::UnsupportedCommand("Send Tab"))?;
        let bundle: SendTabKeysPayload = serde_json::from_str(command)?;
        let public_keys = bundle.decrypt(&self.ksync, &self.kxcs)?;
        let encrypted_payload = send_tab_payload.encrypt(public_keys)?;
        Ok(serde_json::to_value(&encrypted_payload)?)
    }
}

impl CommandsHandler for SendTab {
    fn command_name() -> String {
        COMMAND_SEND_TAB.to_string()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn init(&mut self, local_data: Option<&str>) -> Result<(String, String)> {
        let send_tab_keys: SendTabKeysInternal = local_data
            .map(|s| serde_json::from_str(s))
            .unwrap_or_else(|| Ok(SendTabKeysInternal::from_random()))?;
        let send_tab_keys_public: SendTabKeysPublic = send_tab_keys.clone().into();
        let encrypted_public_keys = send_tab_keys_public.encrypt(&self.ksync, &self.kxcs)?;
        let command_data = serde_json::to_string(&encrypted_public_keys)?;
        Ok((command_data, serde_json::to_string(&send_tab_keys)?))
    }

    fn handle_command(
        &mut self,
        local_data: &str,
        sender: Option<&ClientInstance>,
        payload: serde_json::Value,
    ) -> Result<()> {
        let own_keys: SendTabKeysInternal = serde_json::from_str(local_data)?;
        let payload: EncryptedSendTabPayload = serde_json::from_value(payload)?;
        let decrypted = payload.decrypt(&own_keys)?;
        for tab in decrypted.entries {
            self.tab_received_callback.call(&tab.title, &tab.url);
        }
        Ok(())
    }
}

pub struct TabReceivedCallback {
    callback_fn: Box<Fn(&str, &str) + Sync + Send + RefUnwindSafe>,
}

impl TabReceivedCallback {
    pub fn new<F>(callback_fn: F) -> Self
    where
        F: Fn(&str, &str) + 'static + Sync + Send + RefUnwindSafe,
    {
        Self {
            callback_fn: Box::new(callback_fn),
        }
    }

    // TODO: add sender (send client instance or name?)
    pub fn call(&self, title: &str, url: &str) {
        (*self.callback_fn)(title, url);
    }
}

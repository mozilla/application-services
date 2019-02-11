/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// The Send Tab functionality is backed by Firefox Accounts device commands.
/// A device shows it can handle "Send Tab" commands by advertising the "open-uri"
/// command in its on own device record.
/// This command data bundle contains a one-time generated `PublicSendTabKeys`
/// (while keeping locally `PrivateSendTabKeys` containing the private key),
/// wrapped by the account oldsync scope `kSync` to form a `SendTabKeysPayload`.
///
/// When a device sends a tab to another, it decrypts that `SendTabKeysPayload` using `kSync`,
/// uses the obtained public key to encrypt the `SendTabPayload` it created that
/// contains the tab to send and finally forms the `EncryptedSendTabPayload` that is
/// then sent to the target device.
use crate::{device::Device, errors::*, scoped_keys::ScopedKey, scopes};
use ece::{
    Aes128GcmEceWebPushImpl, LocalKeyPair, LocalKeyPairImpl, RemoteKeyPairImpl, WebPushParams,
};
use hex;
use serde_derive::*;
use sync15::{EncryptedPayload, KeyBundle};

pub const COMMAND_NAME: &str = "https://identity.mozilla.com/cmd/open-uri";

#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptedSendTabPayload {
    /// URL Safe Base 64 encrypted send-tab payload.
    encrypted: String,
}

impl EncryptedSendTabPayload {
    pub fn decrypt(self, keys: &PrivateSendTabKeys) -> Result<SendTabPayload> {
        let encrypted = base64::decode_config(&self.encrypted, base64::URL_SAFE_NO_PAD)?;
        let private_key = LocalKeyPairImpl::new(&keys.private_key)?;
        let decrypted =
            Aes128GcmEceWebPushImpl::decrypt(&private_key, &keys.auth_secret, &encrypted)?;
        Ok(serde_json::from_slice(&decrypted)?)
    }
}

#[derive(Serialize, Deserialize)]
pub struct SendTabPayload {
    pub entries: Vec<TabHistoryEntry>,
}

impl SendTabPayload {
    pub fn single_tab(title: &str, url: &str) -> Self {
        SendTabPayload {
            entries: vec![TabHistoryEntry {
                title: title.to_string(),
                url: url.to_string(),
            }],
        }
    }
    fn encrypt(&self, keys: PublicSendTabKeys) -> Result<EncryptedSendTabPayload> {
        let bytes = serde_json::to_vec(&self)?;
        let public_key = base64::decode_config(&keys.public_key, base64::URL_SAFE_NO_PAD)?;
        let public_key = RemoteKeyPairImpl::from_raw(&public_key);
        let auth_secret = base64::decode_config(&keys.auth_secret, base64::URL_SAFE_NO_PAD)?;
        let encrypted = Aes128GcmEceWebPushImpl::encrypt(
            &public_key,
            &auth_secret,
            &bytes,
            WebPushParams::default(),
        )?;
        let encrypted = base64::encode_config(&encrypted, base64::URL_SAFE_NO_PAD);
        Ok(EncryptedSendTabPayload { encrypted })
    }
}

#[derive(Serialize, Deserialize)]
pub struct TabHistoryEntry {
    pub title: String,
    pub url: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PrivateSendTabKeys {
    public_key: Vec<u8>,
    private_key: Vec<u8>,
    auth_secret: Vec<u8>,
}

impl PrivateSendTabKeys {
    pub fn from_random() -> Result<Self> {
        let (key_pair, auth_secret) = ece::generate_keypair_and_auth_secret()?;
        let private_key = key_pair.to_raw();
        let public_key = key_pair.pub_as_raw()?;
        Ok(Self {
            public_key,
            private_key,
            auth_secret: auth_secret.to_vec(),
        })
    }
}

#[derive(Serialize, Deserialize)]
struct SendTabKeysPayload {
    /// Hex encoded kid.
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
    fn decrypt(self, scoped_key: &ScopedKey) -> Result<PublicSendTabKeys> {
        let (ksync, kxcs) = extract_oldsync_key_components(scoped_key)?;
        if hex::decode(self.kid)? != kxcs {
            return Err(ErrorKind::MismatchedKeys.into());
        }
        let key = KeyBundle::from_ksync_bytes(&ksync)?;
        let encrypted_bso = EncryptedPayload {
            iv: self.iv,
            hmac: self.hmac,
            ciphertext: self.ciphertext,
        };
        Ok(encrypted_bso.decrypt_and_parse_payload(&key)?)
    }
}

#[derive(Serialize, Deserialize)]
pub struct PublicSendTabKeys {
    /// URL Safe Base 64 encoded push public key.
    #[serde(rename = "publicKey")]
    public_key: String,
    /// URL Safe Base 64 encoded auth secret.
    #[serde(rename = "authSecret")]
    auth_secret: String,
}

impl PublicSendTabKeys {
    fn encrypt(&self, scoped_key: &ScopedKey) -> Result<SendTabKeysPayload> {
        let (ksync, kxcs) = extract_oldsync_key_components(scoped_key)?;
        let key = KeyBundle::from_ksync_bytes(&ksync)?;
        let encrypted_payload = EncryptedPayload::from_cleartext_payload(&key, &self)?;
        Ok(SendTabKeysPayload {
            kid: hex::encode(kxcs),
            iv: encrypted_payload.iv,
            hmac: encrypted_payload.hmac,
            ciphertext: encrypted_payload.ciphertext,
        })
    }
    pub fn as_command_data(&self, scoped_key: &ScopedKey) -> Result<String> {
        let encrypted_public_keys = self.encrypt(scoped_key)?;
        Ok(serde_json::to_string(&encrypted_public_keys)?)
    }
}

impl From<PrivateSendTabKeys> for PublicSendTabKeys {
    fn from(internal: PrivateSendTabKeys) -> Self {
        Self {
            public_key: base64::encode_config(&internal.public_key, base64::URL_SAFE_NO_PAD),
            auth_secret: base64::encode_config(&internal.auth_secret, base64::URL_SAFE_NO_PAD),
        }
    }
}

pub fn build_send_command(
    scoped_key: &ScopedKey,
    target: &Device,
    send_tab_payload: &SendTabPayload,
) -> Result<serde_json::Value> {
    let command = target
        .available_commands
        .get(COMMAND_NAME)
        .ok_or_else(|| ErrorKind::UnsupportedCommand(COMMAND_NAME))?;
    let bundle: SendTabKeysPayload = serde_json::from_str(command)?;
    let public_keys = bundle.decrypt(scoped_key)?;
    let encrypted_payload = send_tab_payload.encrypt(public_keys)?;
    Ok(serde_json::to_value(&encrypted_payload)?)
}

fn extract_oldsync_key_components(oldsync_key: &ScopedKey) -> Result<(Vec<u8>, Vec<u8>)> {
    if oldsync_key.scope != scopes::OLD_SYNC {
        return Err(ErrorKind::IllegalState(
            "Only oldsync scoped keys are supported at the moment.",
        )
        .into());
    }
    let kxcs: &str = oldsync_key.kid.splitn(2, '-').collect::<Vec<_>>()[1];
    let kxcs = base64::decode_config(&kxcs, base64::URL_SAFE_NO_PAD)?;
    let ksync = oldsync_key.key_bytes()?;
    Ok((ksync, kxcs))
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// All commands share the same structs for their crypto-keys.

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::super::device::Device;
use super::super::scopes;
use crate::{Error, Result, ScopedKey};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rc_crypto::ece::{self, EcKeyComponents};
use sync15::{EncryptedPayload, KeyBundle};

#[derive(Serialize, Deserialize, Clone)]
pub(crate) enum VersionedPrivateCommandKeys {
    V1(PrivateCommandKeysV1),
}

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct PrivateCommandKeysV1 {
    p256key: EcKeyComponents,
    auth_secret: Vec<u8>,
}
pub(crate) type PrivateCommandKeys = PrivateCommandKeysV1;

impl PrivateCommandKeys {
    // We define this method so if someone attempts to serialize `PrivateCommandKeys` directly
    // they actually get a serialization of `VersionedPrivateCommandKeys`, which is what we want,
    // because the latter "tags" the version.
    // We should work out how to clean this up to avoid these hacks.
    pub(crate) fn serialize(&self) -> Result<String> {
        Ok(serde_json::to_string(&VersionedPrivateCommandKeys::V1(
            self.clone(),
        ))?)
    }

    pub(crate) fn deserialize(s: &str) -> Result<Self> {
        let versionned: VersionedPrivateCommandKeys = serde_json::from_str(s)?;
        match versionned {
            VersionedPrivateCommandKeys::V1(prv_key) => Ok(prv_key),
        }
    }
}

impl PrivateCommandKeys {
    pub fn from_random() -> Result<Self> {
        rc_crypto::ensure_initialized();
        let (key_pair, auth_secret) = ece::generate_keypair_and_auth_secret()?;
        Ok(Self {
            p256key: key_pair.raw_components()?,
            auth_secret: auth_secret.to_vec(),
        })
    }

    pub fn p256key(&self) -> &EcKeyComponents {
        &self.p256key
    }

    pub fn auth_secret(&self) -> &[u8] {
        &self.auth_secret
    }
}

#[derive(Serialize, Deserialize)]
struct CommandKeysPayload {
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

impl CommandKeysPayload {
    fn decrypt(self, scoped_key: &ScopedKey) -> Result<PublicCommandKeys> {
        let (ksync, kxcs) = extract_oldsync_key_components(scoped_key)?;
        if hex::decode(self.kid)? != kxcs {
            return Err(Error::MismatchedKeys);
        }
        let key = KeyBundle::from_ksync_bytes(&ksync)?;
        let encrypted_payload = EncryptedPayload {
            iv: self.iv,
            hmac: self.hmac,
            ciphertext: self.ciphertext,
        };
        Ok(encrypted_payload.decrypt_into(&key)?)
    }
}

#[derive(Serialize, Deserialize)]
pub struct PublicCommandKeys {
    /// URL Safe Base 64 encoded push public key.
    #[serde(rename = "publicKey")]
    public_key: String,
    /// URL Safe Base 64 encoded auth secret.
    #[serde(rename = "authSecret")]
    auth_secret: String,
}

impl PublicCommandKeys {
    fn encrypt(&self, scoped_key: &ScopedKey) -> Result<CommandKeysPayload> {
        let (ksync, kxcs) = extract_oldsync_key_components(scoped_key)?;
        let key = KeyBundle::from_ksync_bytes(&ksync)?;
        let encrypted_payload = EncryptedPayload::from_cleartext_payload(&key, &self)?;
        Ok(CommandKeysPayload {
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
    pub(crate) fn public_key(&self) -> &str {
        &self.public_key
    }
    pub(crate) fn auth_secret(&self) -> &str {
        &self.auth_secret
    }
}

impl From<PrivateCommandKeys> for PublicCommandKeys {
    fn from(internal: PrivateCommandKeys) -> Self {
        Self {
            public_key: URL_SAFE_NO_PAD.encode(internal.p256key.public_key()),
            auth_secret: URL_SAFE_NO_PAD.encode(&internal.auth_secret),
        }
    }
}

fn extract_oldsync_key_components(oldsync_key: &ScopedKey) -> Result<(Vec<u8>, Vec<u8>)> {
    if oldsync_key.scope != scopes::OLD_SYNC {
        return Err(Error::IllegalState(
            "Only oldsync scoped keys are supported at the moment.",
        ));
    }
    let kxcs: &str = oldsync_key.kid.splitn(2, '-').collect::<Vec<_>>()[1];
    let kxcs = URL_SAFE_NO_PAD.decode(kxcs)?;
    let ksync = oldsync_key.key_bytes()?;
    Ok((ksync, kxcs))
}

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedCommandPayload {
    /// URL Safe Base 64 encrypted send-tab payload.
    encrypted: String,
}

impl EncryptedCommandPayload {
    pub(crate) fn decrypt<T: DeserializeOwned>(self, keys: &PrivateCommandKeys) -> Result<T> {
        rc_crypto::ensure_initialized();
        let encrypted = URL_SAFE_NO_PAD.decode(self.encrypted)?;
        let decrypted = ece::decrypt(keys.p256key(), keys.auth_secret(), &encrypted)?;
        Ok(serde_json::from_slice(&decrypted)?)
    }
}

fn encrypt_payload<T: Serialize>(
    payload: &T,
    keys: PublicCommandKeys,
) -> Result<EncryptedCommandPayload> {
    rc_crypto::ensure_initialized();
    let bytes = serde_json::to_vec(payload)?;
    let public_key = URL_SAFE_NO_PAD.decode(keys.public_key())?;
    let auth_secret = URL_SAFE_NO_PAD.decode(keys.auth_secret())?;
    let encrypted = ece::encrypt(&public_key, &auth_secret, &bytes)?;
    let encrypted = URL_SAFE_NO_PAD.encode(encrypted);
    Ok(EncryptedCommandPayload { encrypted })
}

/// encrypt a command suitable for sending via a push message to another device.
pub(crate) fn encrypt_command<T: Serialize>(
    scoped_key: &ScopedKey,
    target: &Device,
    command: &'static str,
    payload: &T,
) -> Result<serde_json::Value> {
    let public_keys = get_public_keys(scoped_key, target, command)?;
    let encrypted_payload = encrypt_payload(payload, public_keys)?;
    Ok(serde_json::to_value(encrypted_payload)?)
}

/// Get the public keys for a command for a device. These are encrypted in a device record.
pub(crate) fn get_public_keys(
    scoped_key: &ScopedKey,
    target: &Device,
    command: &'static str,
) -> Result<PublicCommandKeys> {
    let command = target
        .available_commands
        .get(command)
        .ok_or(Error::UnsupportedCommand(command))?;
    let bundle: CommandKeysPayload = serde_json::from_str(command)?;
    bundle.decrypt(scoped_key)
}

/// decrypt a command sent from another device.
pub(crate) fn decrypt_command<T: DeserializeOwned>(
    v: serde_json::Value,
    keys: &PrivateCommandKeys,
) -> Result<T> {
    let encrypted_payload: EncryptedCommandPayload = serde_json::from_value(v)?;
    encrypted_payload.decrypt(keys)
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rc_crypto::ece;
use serde_derive::*;

use crate::{Error, Result, ScopedKey};

use super::{
    super::device::Device,
    send_tab::{PrivateSendTabKeysV1, PublicSendTabKeys, SendTabKeysPayload},
};

pub const COMMAND_NAME: &str = "https://identity.mozilla.com/cmd/close-uri/v1";

#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptedCloseTabsPayload {
    /// URL Safe Base 64 encrypted payload.
    encrypted: String,
}

impl EncryptedCloseTabsPayload {
    pub(crate) fn decrypt(self, keys: &PrivateSendTabKeysV1) -> Result<CloseTabsPayload> {
        rc_crypto::ensure_initialized();
        let encrypted = URL_SAFE_NO_PAD.decode(self.encrypted)?;
        let decrypted = ece::decrypt(keys.p256key(), keys.auth_secret(), &encrypted)?;
        Ok(serde_json::from_slice(&decrypted)?)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CloseTabsPayload {
    pub urls: Vec<String>,
}

impl From<CloseTabsPayload> for crate::CloseTabsPayload {
    fn from(payload: CloseTabsPayload) -> Self {
        crate::CloseTabsPayload { urls: payload.urls }
    }
}

impl CloseTabsPayload {
    fn encrypt(&self, keys: PublicSendTabKeys) -> Result<EncryptedCloseTabsPayload> {
        rc_crypto::ensure_initialized();
        let bytes = serde_json::to_vec(&self)?;
        let public_key = URL_SAFE_NO_PAD.decode(keys.public_key())?;
        let auth_secret = URL_SAFE_NO_PAD.decode(keys.auth_secret())?;
        let encrypted = ece::encrypt(&public_key, &auth_secret, &bytes)?;
        let encrypted = URL_SAFE_NO_PAD.encode(encrypted);
        Ok(EncryptedCloseTabsPayload { encrypted })
    }
}

pub fn build_close_tabs_command(
    scoped_key: &ScopedKey,
    target: &Device,
    payload: &CloseTabsPayload,
) -> Result<serde_json::Value> {
    let command = target
        .available_commands
        .get(COMMAND_NAME)
        .ok_or(Error::UnsupportedCommand(COMMAND_NAME))?;
    let bundle: SendTabKeysPayload = serde_json::from_str(command)?;
    let public_keys = bundle.decrypt(scoped_key)?;
    let encrypted_payload = payload.encrypt(public_keys)?;
    Ok(serde_json::to_value(encrypted_payload)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::Result;

    #[test]
    fn test_empty_payload() -> Result<()> {
        let empty = r#"{ "urls": []}"#;
        let payload: CloseTabsPayload = serde_json::from_str(empty)?;
        assert!(payload.urls.is_empty());

        Ok(())
    }

    #[test]
    fn test_payload() -> Result<()> {
        let payload = CloseTabsPayload {
            urls: vec!["https://www.mozilla.org".into()],
        };
        let json = serde_json::to_string(&payload)?;
        assert!(!json.is_empty());
        let deserialized: CloseTabsPayload = serde_json::from_str(&json)?;
        assert_eq!(deserialized, payload);

        Ok(())
    }
}

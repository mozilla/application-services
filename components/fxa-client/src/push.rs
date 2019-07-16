/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{error::*, AccountEvent, FirefoxAccount};
use serde_derive::Deserialize;

impl FirefoxAccount {
    /// Handle any incoming push message payload coming from the Firefox Accounts
    /// servers that has been decrypted and authenticated by the Push crate.
    ///
    /// Due to iOS platform restrictions, a push notification must always show UI,
    /// and therefore we only retrieve 1 command per message.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn handle_push_message(&mut self, payload: &str) -> Result<Vec<AccountEvent>> {
        let payload = serde_json::from_str(payload)?;
        match payload {
            PushPayload::CommandReceived(CommandReceivedPushPayload { index, .. }) => {
                if cfg!(target_os = "ios") {
                    self.fetch_device_command(index).map(|cmd| vec![cmd])
                } else {
                    self.poll_device_commands()
                }
            }
            PushPayload::ProfileUpdated => Ok(vec![AccountEvent::ProfileUpdated]),
            _ => {
                // XXX: Handle other types of push payloads.
                log::info!("Ignoring push message {:?}", payload);
                Ok(vec![])
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "command", content = "data")]
pub enum PushPayload {
    #[serde(rename = "fxaccounts:command_received")]
    CommandReceived(CommandReceivedPushPayload),
    #[serde(rename = "fxaccounts:profile_updated")]
    ProfileUpdated,
    #[serde(rename = "fxaccounts:device_connected")]
    DeviceConnected(DeviceConnectedPushPayload),
    #[serde(rename = "fxaccounts:device_disconnected")]
    DeviceDisconnected(DeviceDisconnectedPushPayload),
    #[serde(rename = "fxaccounts:password_changed")]
    PasswordChanged,
    #[serde(rename = "fxaccounts:password_reset")]
    PasswordReset,
    #[serde(rename = "fxaccounts:account_destroyed")]
    AccountDestroyed(AccountDestroyedPushPayload),
}

#[derive(Debug, Deserialize)]
pub struct CommandReceivedPushPayload {
    command: String,
    index: u64,
    sender: String,
    url: String,
}

#[derive(Debug, Deserialize)]
pub struct DeviceConnectedPushPayload {
    #[serde(rename = "deviceName")]
    device_name: String,
}

#[derive(Debug, Deserialize)]
pub struct DeviceDisconnectedPushPayload {
    #[serde(rename = "id")]
    device_id: String,
}

#[derive(Debug, Deserialize)]
pub struct AccountDestroyedPushPayload {
    #[serde(rename = "uid")]
    account_uid: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_deserialize_push_message() {
        let json = "{\"version\":1,\"command\":\"fxaccounts:command_received\",\"data\":{\"command\":\"send-tab-recv\",\"index\":1,\"sender\":\"bobo\",\"url\":\"https://mozilla.org\"}}";
        let _: PushPayload = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_deserialize_empty_push_message() {
        let json = "{\"version\":1,\"command\":\"fxaccounts:profile_updated\"}";
        let _: PushPayload = serde_json::from_str(&json).unwrap();
    }
}

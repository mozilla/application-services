/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::convert::TryInto;

use super::device::CommandFetchReason;
use super::{error::*, FirefoxAccount};
use crate::AccountEvent;
use serde_derive::Deserialize;

impl FirefoxAccount {
    /// Handle any incoming push message payload coming from the Firefox Accounts
    /// servers that has been decrypted and authenticated by the Push crate.
    ///
    /// ** ⚠️ Due to iOS platform restrictions, a push notification must always show UI. **
    /// iOS callers of this API need to ensure that if this returns an empty list
    /// they display some type error notification.
    ///
    /// This API could return an empty list if:
    ///  1. The user sends themselves multiple tabs, tab A and tab B
    ///  2. The device receives the notification for tab A, and queries FxA
    ///  3. The device gets **both** tab A and tab B from FxA
    ///  4. The device handles both tabs
    ///  5. The device receives the notification for tab B (late)
    ///  6. The device queries FxA again, and the tab is already gone!
    ///
    /// We leave handling of the above case to the caller
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn handle_push_message(&mut self, payload: &str) -> Result<Vec<AccountEvent>> {
        let payload = serde_json::from_str(payload).or_else(|err| {
            // Due to a limitation of serde (https://github.com/serde-rs/serde/issues/1714)
            // we can't parse some payloads with an unknown "command" value. Try doing a
            // less-strongly-validating parse so we can silently ignore such messages, while
            // while reporting errors if the payload is completely unintelligible.
            let v: serde_json::Value = serde_json::from_str(payload)?;
            match v.get("command") {
                Some(_) => Ok(PushPayload::Unknown),
                None => Err(err),
            }
        })?;
        match payload {
            PushPayload::CommandReceived(CommandReceivedPushPayload { index, .. }) => {
                let cmds = self.poll_device_commands(CommandFetchReason::Push(index))?;
                cmds.into_iter()
                    .map(|command| {
                        Ok(AccountEvent::CommandReceived {
                            command: command.try_into()?,
                        })
                    })
                    .collect()
            }
            PushPayload::ProfileUpdated => {
                self.state.last_seen_profile = None;
                Ok(vec![AccountEvent::ProfileUpdated])
            }
            PushPayload::DeviceConnected(DeviceConnectedPushPayload { device_name }) => {
                self.clear_devices_and_attached_clients_cache();
                Ok(vec![AccountEvent::DeviceConnected { device_name }])
            }
            PushPayload::DeviceDisconnected(DeviceDisconnectedPushPayload { device_id }) => {
                let local_device = self.get_current_device_id();
                let is_local_device = match local_device {
                    Err(_) => false,
                    Ok(id) => id == device_id,
                };
                if is_local_device {
                    // Note: self.disconnect calls self.start_over which clears the state for the FirefoxAccount instance
                    self.disconnect();
                }
                Ok(vec![AccountEvent::DeviceDisconnected {
                    device_id,
                    is_local_device,
                }])
            }
            PushPayload::AccountDestroyed(AccountDestroyedPushPayload { account_uid }) => {
                let is_local_account = match &self.state.last_seen_profile {
                    None => false,
                    Some(profile) => profile.response.uid == account_uid,
                };
                Ok(if is_local_account {
                    vec![AccountEvent::AccountDestroyed]
                } else {
                    vec![]
                })
            }
            PushPayload::PasswordChanged | PushPayload::PasswordReset => {
                let status = self.check_authorization_status()?;
                // clear any device or client data due to password change.
                self.clear_devices_and_attached_clients_cache();
                Ok(if !status.active {
                    vec![AccountEvent::AccountAuthStateChanged]
                } else {
                    vec![]
                })
            }
            PushPayload::Unknown => {
                log::info!("Unknown Push command.");
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
    #[serde(other)]
    Unknown,
}

// Some of this structs fields are not read, except
// when deserialized, we mark them as dead_code
#[allow(dead_code)]
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
    use crate::internal::http_client::FxAClientMock;
    use crate::internal::http_client::IntrospectResponse;
    use crate::internal::oauth::RefreshToken;
    use crate::internal::util;
    use crate::internal::CachedResponse;
    use crate::internal::Config;
    use std::sync::Arc;

    #[test]
    fn test_deserialize_send_tab_command() {
        let json = "{\"version\":1,\"command\":\"fxaccounts:command_received\",\"data\":{\"command\":\"send-tab-recv\",\"index\":1,\"sender\":\"bobo\",\"url\":\"https://mozilla.org\"}}";
        let _: PushPayload = serde_json::from_str(json).unwrap();
    }

    #[test]
    fn test_push_profile_updated() {
        let mut fxa = FirefoxAccount::with_config(crate::internal::Config::stable_dev(
            "12345678",
            "https://foo.bar",
        ));
        fxa.add_cached_profile("123", "test@example.com", util::now());
        let json = "{\"version\":1,\"command\":\"fxaccounts:profile_updated\"}";
        let events = fxa.handle_push_message(json).unwrap();
        assert!(fxa.state.last_seen_profile.is_none());
        assert_eq!(events.len(), 1);
        match events[0] {
            AccountEvent::ProfileUpdated => {}
            _ => unreachable!(),
        };
    }

    #[test]
    fn test_push_device_disconnected_local() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));
        let refresh_token_scopes = std::collections::HashSet::new();
        fxa.state.refresh_token = Some(crate::internal::oauth::RefreshToken {
            token: "refresh_token".to_owned(),
            scopes: refresh_token_scopes,
        });
        fxa.state.current_device_id = Some("my_id".to_owned());
        let json = "{\"version\":1,\"command\":\"fxaccounts:device_disconnected\",\"data\":{\"id\":\"my_id\"}}";
        let events = fxa.handle_push_message(json).unwrap();
        assert!(fxa.state.refresh_token.is_none());
        assert_eq!(events.len(), 1);
        match &events[0] {
            AccountEvent::DeviceDisconnected {
                device_id,
                is_local_device,
            } => {
                assert!(is_local_device);
                assert_eq!(device_id, "my_id");
            }
            _ => unreachable!(),
        };
    }

    #[test]
    fn test_push_password_reset() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));
        let mut client = FxAClientMock::new();
        client
            .expect_check_refresh_token_status(mockiato::Argument::any, |token| {
                token.partial_eq("refresh_token")
            })
            .times(1)
            .returns_once(Ok(IntrospectResponse { active: true }));
        fxa.set_client(Arc::new(client));
        let refresh_token_scopes = std::collections::HashSet::new();
        fxa.state.refresh_token = Some(RefreshToken {
            token: "refresh_token".to_owned(),
            scopes: refresh_token_scopes,
        });
        fxa.state.current_device_id = Some("my_id".to_owned());
        fxa.devices_cache = Some(CachedResponse {
            response: vec![],
            cached_at: 0,
            etag: "".to_string(),
        });
        let json = "{\"version\":1,\"command\":\"fxaccounts:password_reset\"}";
        assert!(fxa.devices_cache.is_some());
        fxa.handle_push_message(json).unwrap();
        assert!(fxa.devices_cache.is_none());
    }

    #[test]
    fn test_push_device_disconnected_remote() {
        let mut fxa = FirefoxAccount::with_config(crate::internal::Config::stable_dev(
            "12345678",
            "https://foo.bar",
        ));
        let json = "{\"version\":1,\"command\":\"fxaccounts:device_disconnected\",\"data\":{\"id\":\"remote_id\"}}";
        let events = fxa.handle_push_message(json).unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            AccountEvent::DeviceDisconnected {
                device_id,
                is_local_device,
            } => {
                assert!(!is_local_device);
                assert_eq!(device_id, "remote_id");
            }
            _ => unreachable!(),
        };
    }

    #[test]
    fn test_handle_push_message_ignores_unknown_command() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));
        let json = "{\"version\":1,\"command\":\"huh\"}";
        let events = fxa.handle_push_message(json).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_handle_push_message_ignores_unknown_command_with_data() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));
        let json = "{\"version\":1,\"command\":\"huh\",\"data\":{\"value\":42}}";
        let events = fxa.handle_push_message(json).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_handle_push_message_errors_on_garbage_data() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));
        let json = "{\"wtf\":\"bbq\"}";
        fxa.handle_push_message(json).unwrap_err();
    }
}

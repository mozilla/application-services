/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::convert::TryInto;

use super::FirefoxAccount;
use crate::{info, AccountEvent, Error, Result};
use serde_derive::Deserialize;

impl FirefoxAccount {
    /// Handles a push message and returns a single [`AccountEvent`]
    ///
    /// This API is useful for when the app would like to get the AccountEvent associated
    /// with the push message, but would **not** like to retrieve missed commands while doing so.
    ///
    /// **💾 This method alters the persisted account state.**
    ///
    /// **⚠️ This API does not increment the command index if a command was received**
    pub fn handle_push_message(&mut self, payload: &str) -> Result<AccountEvent> {
        let payload = serde_json::from_str(payload).or_else(|err| {
            let v: serde_json::Value = serde_json::from_str(payload)?;
            match v.get("command") {
                Some(_) => Ok(PushPayload::Unknown),
                None => Err(err),
            }
        })?;
        match payload {
            PushPayload::CommandReceived(CommandReceivedPushPayload { index, .. }) => {
                let cmd = self.get_command_for_index(index)?;
                Ok(AccountEvent::CommandReceived {
                    command: cmd.try_into()?,
                })
            }
            PushPayload::ProfileUpdated => {
                self.state.clear_last_seen_profile();
                Ok(AccountEvent::ProfileUpdated)
            }
            PushPayload::DeviceConnected(DeviceConnectedPushPayload { device_name }) => {
                self.clear_devices_and_attached_clients_cache();
                Ok(AccountEvent::DeviceConnected { device_name })
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
                Ok(AccountEvent::DeviceDisconnected {
                    device_id,
                    is_local_device,
                })
            }
            PushPayload::AccountDestroyed(AccountDestroyedPushPayload { account_uid }) => {
                let is_local_account = match self.state.last_seen_profile() {
                    None => false,
                    Some(profile) => profile.response.uid == account_uid,
                };
                Ok(if is_local_account {
                    AccountEvent::AccountDestroyed
                } else {
                    return Err(Error::InvalidPushEvent);
                })
            }
            PushPayload::PasswordChanged | PushPayload::PasswordReset => {
                let status = self.check_authorization_status()?;
                // clear any device or client data due to password change.
                self.clear_devices_and_attached_clients_cache();
                Ok(if !status.active {
                    AccountEvent::AccountAuthStateChanged
                } else {
                    info!("Password change event, but no action required");
                    AccountEvent::Unknown
                })
            }
            PushPayload::Unknown => {
                info!("Unknown Push command.");
                Ok(AccountEvent::Unknown)
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
    use crate::internal::http_client::IntrospectResponse;
    use crate::internal::http_client::MockFxAClient;
    use crate::internal::oauth::RefreshToken;
    use crate::internal::CachedResponse;
    use crate::internal::Config;
    use mockall::predicate::always;
    use mockall::predicate::eq;
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
        fxa.add_cached_profile("123", "test@example.com");
        let json = "{\"version\":1,\"command\":\"fxaccounts:profile_updated\"}";
        let event = fxa.handle_push_message(json).unwrap();
        assert!(fxa.state.last_seen_profile().is_none());
        assert!(matches!(event, AccountEvent::ProfileUpdated));
    }

    #[test]
    fn test_push_device_disconnected_local() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));
        let refresh_token_scopes = std::collections::HashSet::new();
        fxa.state
            .force_refresh_token(crate::internal::oauth::RefreshToken {
                token: "refresh_token".to_owned(),
                scopes: refresh_token_scopes,
            });
        fxa.state.force_current_device_id("my_id");
        let json = "{\"version\":1,\"command\":\"fxaccounts:device_disconnected\",\"data\":{\"id\":\"my_id\"}}";
        let event = fxa.handle_push_message(json).unwrap();
        assert!(fxa.state.refresh_token().is_none());
        match event {
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
        let mut client = MockFxAClient::new();
        client
            .expect_check_refresh_token_status()
            .with(always(), eq("refresh_token"))
            .times(1)
            .returning(|_, _| Ok(IntrospectResponse { active: false }));
        fxa.set_client(Arc::new(client));
        let refresh_token_scopes = std::collections::HashSet::new();
        fxa.state.force_refresh_token(RefreshToken {
            token: "refresh_token".to_owned(),
            scopes: refresh_token_scopes,
        });
        fxa.state.force_current_device_id("my_id");
        fxa.devices_cache = Some(CachedResponse {
            response: vec![],
            cached_at: 0,
            etag: "".to_string(),
        });
        let json = "{\"version\":1,\"command\":\"fxaccounts:password_reset\"}";
        assert!(fxa.devices_cache.is_some());
        let event = fxa.handle_push_message(json).unwrap();
        assert!(matches!(event, AccountEvent::AccountAuthStateChanged));
        assert!(fxa.devices_cache.is_none());
    }

    #[test]
    fn test_push_password_change() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));
        let mut client = MockFxAClient::new();
        client
            .expect_check_refresh_token_status()
            .with(always(), eq("refresh_token"))
            .times(1)
            .returning(|_, _| Ok(IntrospectResponse { active: true }));
        fxa.set_client(Arc::new(client));
        let refresh_token_scopes = std::collections::HashSet::new();
        fxa.state.force_refresh_token(RefreshToken {
            token: "refresh_token".to_owned(),
            scopes: refresh_token_scopes,
        });
        fxa.state.force_current_device_id("my_id");
        fxa.devices_cache = Some(CachedResponse {
            response: vec![],
            cached_at: 0,
            etag: "".to_string(),
        });
        let json = "{\"version\":1,\"command\":\"fxaccounts:password_changed\"}";
        assert!(fxa.devices_cache.is_some());
        let event = fxa.handle_push_message(json).unwrap();
        assert!(matches!(event, AccountEvent::Unknown));
        assert!(fxa.devices_cache.is_none());
    }
    #[test]
    fn test_push_device_disconnected_remote() {
        let mut fxa = FirefoxAccount::with_config(crate::internal::Config::stable_dev(
            "12345678",
            "https://foo.bar",
        ));
        let json = "{\"version\":1,\"command\":\"fxaccounts:device_disconnected\",\"data\":{\"id\":\"remote_id\"}}";
        let event = fxa.handle_push_message(json).unwrap();
        match event {
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
        let event = fxa.handle_push_message(json).unwrap();
        assert!(matches!(event, AccountEvent::Unknown));
    }

    #[test]
    fn test_handle_push_message_ignores_unknown_command_with_data() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));
        let json = "{\"version\":1,\"command\":\"huh\",\"data\":{\"value\":42}}";
        let event = fxa.handle_push_message(json).unwrap();
        assert!(matches!(event, AccountEvent::Unknown));
    }

    #[test]
    fn test_handle_push_message_errors_on_garbage_data() {
        let mut fxa =
            FirefoxAccount::with_config(Config::stable_dev("12345678", "https://foo.bar"));
        let json = "{\"wtf\":\"bbq\"}";
        fxa.handle_push_message(json).unwrap_err();
    }
}

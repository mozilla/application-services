/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::mem;

use payload_support::Fit;

use super::{
    commands::{
        close_tabs::{self, CloseTabsPayload},
        decrypt_command, encrypt_command, IncomingDeviceCommand, PrivateCommandKeys,
    },
    device::COMMAND_MAX_PAYLOAD_SIZE,
    http_client::GetDeviceResponse,
    scopes, telemetry, FirefoxAccount,
};
use crate::{warn, CloseTabsResult, Error, Result};

impl FirefoxAccount {
    pub fn close_tabs<T>(&mut self, target_device_id: &str, urls: Vec<T>) -> Result<CloseTabsResult>
    where
        T: Into<String>,
    {
        let devices = self.get_devices(false)?;
        let target = devices
            .iter()
            .find(|d| d.id == target_device_id)
            .ok_or_else(|| Error::UnknownTargetDevice(target_device_id.to_owned()))?;

        let sent_telemetry = telemetry::SentCommand::for_close_tabs();
        let mut urls_to_retry = Vec::new();

        // Sort the URLs shortest to longest, so that we can at least make
        // some forward progress, even if there's an oversized URL at the
        // end that won't fit into a single command.
        let mut urls: Vec<_> = urls.into_iter().map(Into::into).collect();
        urls.sort_unstable_by_key(String::len);

        while !urls.is_empty() {
            // If we were asked to close more URLs than will fit in a
            // single command, chunk the URLs into multiple commands,
            // packing as many as we can into each. Do this until we've either
            // drained and packed all the URLs, or we see an oversized URL
            // that won't fit into a single command.
            let chunk = match payload_support::try_fit_items(&urls, COMMAND_MAX_PAYLOAD_SIZE.get())
            {
                Fit::All => mem::take(&mut urls),
                Fit::Some(count) => urls.drain(..count.get()).collect(),
                Fit::None | Fit::Err(_) => {
                    // Oversized URLs that won't fit into a single command, and
                    // serialization errors, are permanent; retrying to send
                    // these URLs won't help. But we want our consumers to keep
                    // any pending closed URLs hidden from the user's synced
                    // tabs list, until they're eventually sent (for temporary
                    // errors; see below), or expire after some time
                    // (for oversized URLs that can't ever be sent).
                    urls_to_retry.append(&mut urls);
                    break;
                }
            };

            let sent_telemetry = sent_telemetry.clone_with_new_stream_id();
            let payload = CloseTabsPayload::with_telemetry(&sent_telemetry, chunk);

            let oldsync_key = self.get_scoped_key(scopes::OLD_SYNC)?;
            let command_payload =
                encrypt_command(oldsync_key, target, close_tabs::COMMAND_NAME, &payload)?;
            let result = self.invoke_command(
                close_tabs::COMMAND_NAME,
                target,
                &command_payload,
                Some(close_tabs::COMMAND_TTL),
            );
            match result {
                Ok(()) => {
                    self.telemetry.record_command_sent(sent_telemetry);
                }
                Err(e) => {
                    error_support::report_error!(
                        "fxaclient-close-tabs-invoke",
                        "Failed to send bulk Close Tabs command: {}",
                        e
                    );
                    // Temporary error; if the consumer retries, we expect that
                    // we _will_ eventually send these URLs.
                    urls_to_retry.extend(payload.urls);
                }
            }
        }

        Ok(if urls_to_retry.is_empty() {
            CloseTabsResult::Ok
        } else {
            CloseTabsResult::TabsNotClosed {
                urls: urls_to_retry,
            }
        })
    }

    pub(crate) fn handle_close_tabs_command(
        &mut self,
        sender: Option<GetDeviceResponse>,
        payload: serde_json::Value,
        reason: telemetry::ReceivedReason,
    ) -> Result<IncomingDeviceCommand> {
        let close_tabs_key: PrivateCommandKeys = match self.close_tabs_key() {
            Some(s) => PrivateCommandKeys::deserialize(s)?,
            None => {
                return Err(Error::IllegalState(
                    "Cannot find Close Remote Tabs keys. Has initialize_device been called before?",
                ));
            }
        };
        match decrypt_command(payload, &close_tabs_key) {
            Ok(payload) => {
                let recd_telemetry = telemetry::ReceivedCommand::for_close_tabs(&payload, reason);
                self.telemetry.record_command_received(recd_telemetry);
                Ok(IncomingDeviceCommand::TabsClosed { sender, payload })
            }
            Err(e) => {
                warn!("Could not decrypt Close Remote Tabs payload. Diagnosing then resetting the Close Tabs keys.");
                self.clear_close_tabs_keys();
                self.reregister_current_capabilities()?;
                Err(e)
            }
        }
    }

    pub(crate) fn load_or_generate_close_tabs_keys(&mut self) -> Result<PrivateCommandKeys> {
        if let Some(s) = self.close_tabs_key() {
            match PrivateCommandKeys::deserialize(s) {
                Ok(keys) => return Ok(keys),
                Err(_) => {
                    error_support::report_error!(
                        "fxaclient-close-tabs-key-deserialize",
                        "Could not deserialize Close Remote Tabs keys. Re-creating them."
                    );
                }
            }
        }
        let keys = PrivateCommandKeys::from_random()?;
        self.set_close_tabs_key(keys.serialize()?);
        Ok(keys)
    }

    fn close_tabs_key(&self) -> Option<&str> {
        self.state.get_commands_data(close_tabs::COMMAND_NAME)
    }

    fn set_close_tabs_key(&mut self, key: String) {
        self.state.set_commands_data(close_tabs::COMMAND_NAME, key)
    }

    fn clear_close_tabs_keys(&mut self) {
        self.state.clear_commands_data(close_tabs::COMMAND_NAME);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{collections::HashSet, sync::Arc};

    use mockall::predicate::{always, eq};
    use nss::ensure_initialized;
    use serde_json::json;

    use crate::{
        internal::{
            commands::PublicCommandKeys, config::Config, http_client::MockFxAClient,
            oauth::RefreshToken, util, CachedResponse, FirefoxAccount,
        },
        ScopedKey,
    };

    /// An RAII helper that overrides the maximum command payload size
    /// for testing, and restores the original size when dropped.
    struct OverrideCommandMaxPayloadSize(usize);

    impl OverrideCommandMaxPayloadSize {
        pub fn with_new_size(new_size: usize) -> Self {
            Self(COMMAND_MAX_PAYLOAD_SIZE.replace(new_size))
        }
    }

    impl Drop for OverrideCommandMaxPayloadSize {
        fn drop(&mut self) {
            COMMAND_MAX_PAYLOAD_SIZE.set(self.0)
        }
    }

    fn setup() -> FirefoxAccount {
        ensure_initialized();
        let config = Config::stable_dev("12345678", "https://foo.bar");
        let mut fxa = FirefoxAccount::with_config(config);
        fxa.state.force_refresh_token(RefreshToken {
            token: "refreshtok".to_owned(),
            scopes: HashSet::default(),
        });
        fxa.state.insert_scoped_key(scopes::OLD_SYNC, ScopedKey {
            kty: "oct".to_string(),
            scope: "https://identity.mozilla.com/apps/oldsync".to_string(),
            k: "kMtwpVC0ZaYFJymPza8rXK_0CgCp3KMwRStwGfBRBDtL6hXRDVJgQFaoOQ2dimw0Bko5WVv2gNTy7RX5zFYZHg".to_string(),
            kid: "1542236016429-Ox1FbJfFfwTe5t-xq4v2hQ".to_string(),
        });
        fxa
    }

    // Quasi-integration tests that stub out _just_ enough of the
    // machinery to send and respond to "close tabs" commands.

    #[test]
    fn test_close_tabs_send_one() -> Result<()> {
        let _o = OverrideCommandMaxPayloadSize::with_new_size(2048);

        let mut fxa = setup();
        let close_tabs_keys = PrivateCommandKeys::from_random()?;
        let devices = json!([
            {
                "id": "device0102",
                "name": "Emerald",
                "isCurrentDevice": false,
                "location": {},
                "availableCommands": {
                    close_tabs::COMMAND_NAME: PublicCommandKeys::as_command_data(
                        &close_tabs_keys.clone().into(),
                        fxa.state.get_scoped_key(scopes::OLD_SYNC).unwrap(),
                    )?,
                },
                "pushEndpointExpired": false,
            },
        ]);
        fxa.devices_cache = Some(CachedResponse {
            response: serde_json::from_value(devices)?,
            cached_at: util::now(),
            etag: "".into(),
        });
        fxa.set_close_tabs_key(close_tabs_keys.serialize()?);

        let mut client = MockFxAClient::new();
        client
            .expect_invoke_command()
            .once()
            .with(
                always(),
                always(),
                always(),
                eq("device0102"),
                always(),
                always(),
            )
            .returning(|_, _, _, _, _, _| Ok(()));
        fxa.set_client(Arc::new(client));

        // Send one command.
        assert_eq!(
            fxa.close_tabs("device0102", vec!["https://example.com"])?,
            CloseTabsResult::Ok
        );

        Ok(())
    }

    #[test]
    fn test_close_tabs_send_two() -> Result<()> {
        let _o = OverrideCommandMaxPayloadSize::with_new_size(2048);

        let mut fxa = setup();
        let close_tabs_keys = PrivateCommandKeys::from_random()?;
        let devices = json!([
            {
                "id": "device0304",
                "name": "Sapphire",
                "isCurrentDevice": false,
                "location": {},
                "availableCommands": {
                    close_tabs::COMMAND_NAME: PublicCommandKeys::as_command_data(
                        &close_tabs_keys.clone().into(),
                        fxa.state.get_scoped_key(scopes::OLD_SYNC).unwrap(),
                    )?,
                },
                "pushEndpointExpired": false,
            },
        ]);
        fxa.devices_cache = Some(CachedResponse {
            response: serde_json::from_value(devices)?,
            cached_at: util::now(),
            etag: "".into(),
        });
        fxa.set_close_tabs_key(close_tabs_keys.serialize()?);

        let mut client = MockFxAClient::new();
        client
            .expect_invoke_command()
            .times(2)
            .with(
                always(),
                always(),
                always(),
                eq("device0304"),
                always(),
                always(),
            )
            .returning(|_, _, _, _, _, _| Ok(()));
        fxa.set_client(Arc::new(client));

        // Send two commands.
        assert_eq!(
            fxa.close_tabs(
                "device0304",
                vec!["https://example.com", "https://example.org"],
            )?,
            CloseTabsResult::Ok
        );

        Ok(())
    }

    #[test]
    fn test_close_tabs_all_fail() -> Result<()> {
        let _o = OverrideCommandMaxPayloadSize::with_new_size(2048);

        let mut fxa = setup();
        let close_tabs_keys = PrivateCommandKeys::from_random()?;
        let devices = json!([
            {
                "id": "device0506",
                "name": "Ruby",
                "isCurrentDevice": false,
                "location": {},
                "availableCommands": {
                    close_tabs::COMMAND_NAME: PublicCommandKeys::as_command_data(
                        &close_tabs_keys.clone().into(),
                        fxa.state.get_scoped_key(scopes::OLD_SYNC).unwrap(),
                    )?,
                },
                "pushEndpointExpired": false,
            },
        ]);
        fxa.devices_cache = Some(CachedResponse {
            response: serde_json::from_value(devices)?,
            cached_at: util::now(),
            etag: "".into(),
        });
        fxa.set_close_tabs_key(close_tabs_keys.serialize()?);

        let mut client = MockFxAClient::new();
        client
            .expect_invoke_command()
            .times(3)
            .with(
                always(),
                always(),
                always(),
                eq("device0506"),
                always(),
                always(),
            )
            .returning(|_, _, _, _, _, _| {
                Err(Error::RequestError(viaduct::Error::NetworkError(
                    "Simulated error".to_owned(),
                )))
            });
        fxa.set_client(Arc::new(client));

        // Fail to send any commands.
        assert_eq!(
            fxa.close_tabs(
                "device0506",
                vec![
                    "https://example.com",
                    "https://example.org",
                    "https://example.net",
                ],
            )?,
            CloseTabsResult::TabsNotClosed {
                urls: vec![
                    "https://example.com".into(),
                    "https://example.org".into(),
                    "https://example.net".into(),
                ]
            }
        );

        Ok(())
    }

    #[test]
    fn test_close_tabs_one_fails() -> Result<()> {
        let _o = OverrideCommandMaxPayloadSize::with_new_size(2048);

        let mut fxa = setup();
        let close_tabs_keys = PrivateCommandKeys::from_random()?;
        let devices = json!([
            {
                "id": "device0708",
                "name": "Agate",
                "isCurrentDevice": false,
                "location": {},
                "availableCommands": {
                    close_tabs::COMMAND_NAME: PublicCommandKeys::as_command_data(
                        &close_tabs_keys.clone().into(),
                        fxa.state.get_scoped_key(scopes::OLD_SYNC).unwrap(),
                    )?,
                },
                "pushEndpointExpired": false,
            },
        ]);
        fxa.devices_cache = Some(CachedResponse {
            response: serde_json::from_value(devices)?,
            cached_at: util::now(),
            etag: "".into(),
        });
        fxa.set_close_tabs_key(close_tabs_keys.serialize()?);

        let mut client = MockFxAClient::new();
        client
            .expect_invoke_command()
            .times(3)
            .with(
                always(),
                always(),
                always(),
                eq("device0708"),
                always(),
                always(),
            )
            // `.returning()` boxes its closure, so we need to capture
            // the keys by `move`.
            .returning(move |_, _, _, _, value, _| {
                let payload: CloseTabsPayload = decrypt_command(value.clone(), &close_tabs_keys)?;
                if payload.urls.iter().any(|url| url == "https://example.org") {
                    Err(Error::RequestError(viaduct::Error::NetworkError(
                        "Simulated error".to_owned(),
                    )))
                } else {
                    Ok(())
                }
            });
        fxa.set_client(Arc::new(client));

        // Send two commands; fail to send one.
        assert_eq!(
            fxa.close_tabs(
                "device0708",
                vec![
                    "https://example.com",
                    "https://example.org",
                    "https://example.net",
                ],
            )?,
            CloseTabsResult::TabsNotClosed {
                urls: vec!["https://example.org".into()]
            }
        );

        Ok(())
    }

    #[test]
    fn test_close_tabs_never_sent() -> Result<()> {
        // Lower the maximum payload size such that we can't send
        // any commands.
        let _p = OverrideCommandMaxPayloadSize::with_new_size(0);

        let mut fxa = setup();
        let close_tabs_keys = PrivateCommandKeys::from_random()?;
        let devices = json!([
            {
                "id": "device0910",
                "name": "Amethyst",
                "isCurrentDevice": false,
                "location": {},
                "availableCommands": {
                    close_tabs::COMMAND_NAME: PublicCommandKeys::as_command_data(
                        &close_tabs_keys.clone().into(),
                        fxa.state.get_scoped_key(scopes::OLD_SYNC).unwrap(),
                    )?,
                },
                "pushEndpointExpired": false,
            },
        ]);
        fxa.devices_cache = Some(CachedResponse {
            response: serde_json::from_value(devices)?,
            cached_at: util::now(),
            etag: "".into(),
        });
        fxa.set_close_tabs_key(close_tabs_keys.serialize()?);

        let mut client = MockFxAClient::new();
        client.expect_invoke_command().never().with(
            always(),
            always(),
            always(),
            eq("device0910"),
            always(),
            always(),
        );
        fxa.set_client(Arc::new(client));

        assert_eq!(
            fxa.close_tabs("device0910", vec!["https://example.com"])?,
            CloseTabsResult::TabsNotClosed {
                urls: vec!["https://example.com".into()]
            }
        );

        Ok(())
    }

    #[test]
    fn test_close_tabs_two_per_command() -> Result<()> {
        // Raise the maximum payload size to 2 URLs per command.
        let _q = OverrideCommandMaxPayloadSize::with_new_size(2088);

        let mut fxa = setup();
        let close_tabs_keys = PrivateCommandKeys::from_random()?;
        let devices = json!([
            {
                "id": "device1112",
                "name": "Diamond",
                "isCurrentDevice": false,
                "location": {},
                "availableCommands": {
                    close_tabs::COMMAND_NAME: PublicCommandKeys::as_command_data(
                        &close_tabs_keys.clone().into(),
                        fxa.state.get_scoped_key(scopes::OLD_SYNC).unwrap(),
                    )?,
                },
                "pushEndpointExpired": false,
            },
        ]);
        fxa.devices_cache = Some(CachedResponse {
            response: serde_json::from_value(devices)?,
            cached_at: util::now(),
            etag: "".into(),
        });
        fxa.set_close_tabs_key(close_tabs_keys.serialize()?);

        let mut client = MockFxAClient::new();
        client
            .expect_invoke_command()
            .times(2)
            .with(
                always(),
                always(),
                always(),
                eq("device1112"),
                always(),
                always(),
            )
            .returning(|_, _, _, _, _, _| Ok(()));
        fxa.set_client(Arc::new(client));

        assert_eq!(
            fxa.close_tabs(
                "device1112",
                vec![
                    "https://example.com/abcdefghi",
                    "https://example.org/jklmnopqr",
                    "https://example.net/stuvwxyza",
                    "https://example.edu/bcdefghij",
                ],
            )?,
            CloseTabsResult::Ok
        );

        Ok(())
    }
}

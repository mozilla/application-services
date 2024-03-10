/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{
    commands::{
        send_tab::{
            self, EncryptedSendTabPayload, PrivateSendTabKeys, PublicSendTabKeys,
            SendTabKeysPayload, SendTabPayload,
        },
        IncomingDeviceCommand,
    },
    http_client::GetDeviceResponse,
    scopes, telemetry, FirefoxAccount,
};
use crate::{Error, Result};

impl FirefoxAccount {
    /// Generate the Send Tab command to be registered with the server.
    ///
    /// **💾 This method alters the persisted account state.**
    pub(crate) fn generate_send_tab_command_data(&mut self) -> Result<String> {
        let own_keys = self.load_or_generate_keys()?;
        let public_keys: PublicSendTabKeys = own_keys.into();
        let oldsync_key = self.get_scoped_key(scopes::OLD_SYNC)?;
        public_keys.as_command_data(oldsync_key)
    }

    fn load_or_generate_keys(&mut self) -> Result<PrivateSendTabKeys> {
        if let Some(s) = self.send_tab_key() {
            match PrivateSendTabKeys::deserialize(s) {
                Ok(keys) => return Ok(keys),
                Err(_) => {
                    error_support::report_error!(
                        "fxaclient-send-tab-key-deserialize",
                        "Could not deserialize Send Tab keys. Re-creating them."
                    );
                }
            }
        }
        let keys = PrivateSendTabKeys::from_random()?;
        self.set_send_tab_key(keys.serialize()?);
        Ok(keys)
    }

    /// Send a single tab to another device designated by its device ID.
    /// XXX - We need a new send_tabs_to_devices() so we can correctly record
    /// telemetry for these cases.
    /// This probably requires a new "Tab" struct with the title and url.
    /// android-components has SendToAllUseCase(), so this isn't just theoretical.
    /// See <https://github.com/mozilla/application-services/issues/3402>
    pub fn send_single_tab(
        &mut self,
        target_device_id: &str,
        title: &str,
        url: &str,
    ) -> Result<()> {
        let devices = self.get_devices(false)?;
        let target = devices
            .iter()
            .find(|d| d.id == target_device_id)
            .ok_or_else(|| Error::UnknownTargetDevice(target_device_id.to_owned()))?;
        let (payload, sent_telemetry) = SendTabPayload::single_tab(title, url);
        let oldsync_key = self.get_scoped_key(scopes::OLD_SYNC)?;
        let command_payload = send_tab::build_send_command(oldsync_key, target, &payload)?;
        self.invoke_command(send_tab::COMMAND_NAME, target, &command_payload)?;
        self.telemetry.record_tab_sent(sent_telemetry);
        Ok(())
    }

    pub(crate) fn handle_send_tab_command(
        &mut self,
        sender: Option<GetDeviceResponse>,
        payload: serde_json::Value,
        reason: telemetry::ReceivedReason,
    ) -> Result<IncomingDeviceCommand> {
        let send_tab_key: PrivateSendTabKeys = match self.send_tab_key() {
            Some(s) => PrivateSendTabKeys::deserialize(s)?,
            None => {
                return Err(Error::IllegalState(
                    "Cannot find send-tab keys. Has initialize_device been called before?",
                ));
            }
        };
        let encrypted_payload: EncryptedSendTabPayload = serde_json::from_value(payload)?;
        match encrypted_payload.decrypt(&send_tab_key) {
            Ok(payload) => {
                // It's an incoming tab, which we record telemetry for.
                let recd_telemetry = telemetry::ReceivedCommand {
                    flow_id: payload.flow_id.clone(),
                    stream_id: payload.stream_id.clone(),
                    reason,
                };
                self.telemetry.record_tab_received(recd_telemetry);
                // The telemetry IDs escape to the consumer, but that's OK...
                Ok(IncomingDeviceCommand::TabReceived { sender, payload })
            }
            Err(e) => {
                // XXX - this seems ripe for telemetry collection!?
                // It also seems like it might be possible to recover - ie, one
                // of the reasons is that there are key mismatches. Doesn't that
                // mean the "other" key might work?
                log::warn!("Could not decrypt Send Tab payload. Diagnosing then resetting the Send Tab keys.");
                match self.diagnose_remote_keys(send_tab_key) {
                    Ok(_) => {
                        error_support::report_error!(
                            "fxaclient-send-tab-decrypt",
                            "Could not find the cause of the Send Tab keys issue."
                        );
                    }
                    Err(e) => {
                        error_support::report_error!("fxaclient-send-tab-decrypt", "{}", e);
                    }
                };
                // Reset the Send Tab keys.
                self.clear_send_tab_key();
                self.reregister_current_capabilities()?;
                Err(e)
            }
        }
    }

    fn send_tab_key(&self) -> Option<&str> {
        self.state.get_commands_data(send_tab::COMMAND_NAME)
    }

    fn set_send_tab_key(&mut self, key: String) {
        self.state.set_commands_data(send_tab::COMMAND_NAME, key)
    }

    fn clear_send_tab_key(&mut self) {
        self.state.clear_commands_data(send_tab::COMMAND_NAME);
    }

    fn diagnose_remote_keys(&mut self, local_send_tab_key: PrivateSendTabKeys) -> Result<()> {
        let own_device = &mut self
            .get_current_device()?
            .ok_or(Error::SendTabDiagnosisError("No remote device."))?;

        let command = own_device
            .available_commands
            .get(send_tab::COMMAND_NAME)
            .ok_or(Error::SendTabDiagnosisError("No remote command."))?;
        let bundle: SendTabKeysPayload = serde_json::from_str(command)?;
        let oldsync_key = self.get_scoped_key(scopes::OLD_SYNC)?;
        let public_keys_remote = bundle
            .decrypt(oldsync_key)
            .map_err(|_| Error::SendTabDiagnosisError("Unable to decrypt public key bundle."))?;

        let public_keys_local: PublicSendTabKeys = local_send_tab_key.into();

        if public_keys_local.public_key() != public_keys_remote.public_key() {
            return Err(Error::SendTabDiagnosisError("Mismatch in public key."));
        }

        if public_keys_local.auth_secret() != public_keys_remote.auth_secret() {
            return Err(Error::SendTabDiagnosisError("Mismatch in auth secret."));
        }
        Ok(())
    }
}

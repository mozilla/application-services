/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{
    commands::{
        close_all_inactive_tabs::{self, CloseAllInactiveTabsPayload},
        decrypt_command, encrypt_command, IncomingDeviceCommand, PrivateCommandKeys,
    },
    http_client::GetDeviceResponse,
    scopes, telemetry, FirefoxAccount,
};
use crate::{Error, Result};

impl FirefoxAccount {
    pub fn close_all_inactive_tabs(&mut self, target_device_id: &str) -> Result<()> {
        let devices = self.get_devices(false)?;
        let target = devices
            .iter()
            .find(|d| d.id == target_device_id)
            .ok_or_else(|| Error::UnknownTargetDevice(target_device_id.to_owned()))?;
        let (payload, sent_telemetry) = CloseAllInactiveTabsPayload::new();
        let oldsync_key = self.get_scoped_key(scopes::OLD_SYNC)?;
        let command_payload = encrypt_command(
            oldsync_key,
            target,
            close_all_inactive_tabs::COMMAND_NAME,
            &payload,
        )?;
        self.invoke_command(
            close_all_inactive_tabs::COMMAND_NAME,
            target,
            &command_payload,
            Some(close_all_inactive_tabs::COMMAND_TTL),
        )?;
        self.telemetry.record_command_sent(sent_telemetry);
        Ok(())
    }

    pub(crate) fn handle_close_all_inactive_tabs_command(
        &mut self,
        sender: Option<GetDeviceResponse>,
        payload: serde_json::Value,
        reason: telemetry::ReceivedReason,
    ) -> Result<IncomingDeviceCommand> {
        let key: PrivateCommandKeys = match self.close_all_inactive_tabs_key() {
            Some(s) => PrivateCommandKeys::deserialize(s)?,
            None => {
                return Err(Error::IllegalState(
                    "Cannot find remote device keys. Has initialize_device been called before?",
                ));
            }
        };
        match decrypt_command(payload, &key) {
            Ok(payload) => {
                let recd_telemetry =
                    telemetry::ReceivedCommand::for_close_all_inactive_tabs(&payload, reason);
                self.telemetry.record_command_received(recd_telemetry);
                Ok(IncomingDeviceCommand::AllInactiveTabsClosed { sender, payload })
            }
            Err(e) => {
                log::warn!("Could not decrypt payload; resetting the command's keys.");
                self.clear_close_all_inactive_tabs_keys();
                self.reregister_current_capabilities()?;
                Err(e)
            }
        }
    }

    pub(crate) fn load_or_generate_close_all_inactive_tabs_keys(
        &mut self,
    ) -> Result<PrivateCommandKeys> {
        if let Some(s) = self.close_all_inactive_tabs_key() {
            match PrivateCommandKeys::deserialize(s) {
                Ok(keys) => return Ok(keys),
                Err(_) => {
                    error_support::report_error!(
                        "fxaclient-close-inactive-tabs-key-deserialize",
                        "Could not deserialize keys. Re-creating them."
                    );
                }
            }
        }
        let keys = PrivateCommandKeys::from_random()?;
        self.set_close_all_inactive_tabs_key(keys.serialize()?);
        Ok(keys)
    }

    fn close_all_inactive_tabs_key(&self) -> Option<&str> {
        self.state
            .get_commands_data(close_all_inactive_tabs::COMMAND_NAME)
    }

    fn set_close_all_inactive_tabs_key(&mut self, key: String) {
        self.state
            .set_commands_data(close_all_inactive_tabs::COMMAND_NAME, key)
    }

    fn clear_close_all_inactive_tabs_keys(&mut self) {
        self.state
            .clear_commands_data(close_all_inactive_tabs::COMMAND_NAME);
    }
}

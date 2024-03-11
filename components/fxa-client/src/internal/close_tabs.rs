/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{
    commands::{
        close_tabs::{self, CloseTabsPayload, EncryptedCloseTabsPayload},
        send_tab::PrivateSendTabKeys,
        IncomingDeviceCommand,
    },
    http_client::GetDeviceResponse,
    scopes, FirefoxAccount,
};
use crate::{Error, Result};

impl FirefoxAccount {
    pub fn close_tabs<T: AsRef<str>>(&mut self, target_device_id: &str, urls: &[T]) -> Result<()> {
        let devices = self.get_devices(false)?;
        let target = devices
            .iter()
            .find(|d| d.id == target_device_id)
            .ok_or_else(|| Error::UnknownTargetDevice(target_device_id.to_owned()))?;
        let payload = CloseTabsPayload {
            urls: urls.iter().map(|url| url.as_ref().to_owned()).collect(),
        };
        let oldsync_key = self.get_scoped_key(scopes::OLD_SYNC)?;
        let command_payload = close_tabs::build_close_tabs_command(oldsync_key, target, &payload)?;
        self.invoke_command(
            close_tabs::COMMAND_NAME,
            target,
            &command_payload,
            Some(close_tabs::COMMAND_TTL),
        )?;
        Ok(())
    }

    pub(crate) fn handle_close_tabs_command(
        &mut self,
        sender: Option<GetDeviceResponse>,
        payload: serde_json::Value,
    ) -> Result<IncomingDeviceCommand> {
        let send_tab_key: PrivateSendTabKeys = match self.close_tabs_key() {
            Some(s) => PrivateSendTabKeys::deserialize(s)?,
            None => {
                return Err(Error::IllegalState(
                    "Cannot find Close Remote Tabs keys. Has initialize_device been called before?",
                ));
            }
        };
        let encrypted_payload: EncryptedCloseTabsPayload = serde_json::from_value(payload)?;
        match encrypted_payload.decrypt(&send_tab_key) {
            Ok(payload) => Ok(IncomingDeviceCommand::TabsClosed { sender, payload }),
            Err(e) => {
                log::warn!("Could not decrypt Close Remote Tabs payload. Diagnosing then resetting the Close Tabs keys.");
                self.clear_close_tabs_keys();
                self.reregister_current_capabilities()?;
                Err(e)
            }
        }
    }

    pub(crate) fn load_or_generate_close_tabs_keys(&mut self) -> Result<PrivateSendTabKeys> {
        if let Some(s) = self.close_tabs_key() {
            match PrivateSendTabKeys::deserialize(s) {
                Ok(keys) => return Ok(keys),
                Err(_) => {
                    error_support::report_error!(
                        "fxaclient-close-tabs-key-deserialize",
                        "Could not deserialize Close Remote Tabs keys. Re-creating them."
                    );
                }
            }
        }
        let keys = PrivateSendTabKeys::from_random()?;
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

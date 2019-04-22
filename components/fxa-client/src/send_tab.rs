/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub use crate::commands::send_tab::{SendTabPayload, TabHistoryEntry};
use crate::{
    commands::send_tab::{self, EncryptedSendTabPayload, PrivateSendTabKeys, PublicSendTabKeys},
    errors::*,
    http_client::GetDeviceResponse,
    scopes, FirefoxAccount,
};

impl FirefoxAccount {
    /// Generate the Send Tab command to be registered with the server.
    ///
    /// **💾 This method alters the persisted account state.**
    pub(crate) fn generate_send_tab_command_data(&mut self) -> Result<String> {
        let own_keys: PrivateSendTabKeys =
            match self.state.commands_data.get(send_tab::COMMAND_NAME) {
                Some(s) => serde_json::from_str(s)?,
                None => {
                    let keys = PrivateSendTabKeys::from_random()?;
                    self.state.commands_data.insert(
                        send_tab::COMMAND_NAME.to_owned(),
                        serde_json::to_string(&keys)?,
                    );
                    keys
                }
            };
        let public_keys: PublicSendTabKeys = own_keys.into();
        let oldsync_key = self.get_scoped_key(scopes::OLD_SYNC)?;
        public_keys.as_command_data(&oldsync_key)
    }

    /// Send a single tab to another device designated by its device ID.
    pub fn send_tab(&self, target_device_id: &str, title: &str, url: &str) -> Result<()> {
        let devices = self.get_devices()?;
        let target = devices
            .iter()
            .find(|d| d.id == target_device_id)
            .ok_or_else(|| ErrorKind::UnknownTargetDevice(target_device_id.to_owned()))?;
        let payload = SendTabPayload::single_tab(title, url);
        let oldsync_key = self.get_scoped_key(scopes::OLD_SYNC)?;
        let command_payload = send_tab::build_send_command(&oldsync_key, target, &payload)?;
        self.invoke_command(send_tab::COMMAND_NAME, target, &command_payload)
    }

    pub(crate) fn handle_send_tab_command(
        &self,
        sender: Option<GetDeviceResponse>,
        payload: serde_json::Value,
    ) -> Result<(Option<GetDeviceResponse>, SendTabPayload)> {
        let send_tab_key: PrivateSendTabKeys =
            match self.state.commands_data.get(send_tab::COMMAND_NAME) {
                Some(s) => serde_json::from_str(s)?,
                None => {
                    return Err(ErrorKind::IllegalState(
                        "Cannot find send-tab keys. Has initialize_device been called before?",
                    )
                    .into());
                }
            };
        let encrypted_payload: EncryptedSendTabPayload = serde_json::from_value(payload)?;
        Ok((sender, encrypted_payload.decrypt(&send_tab_key)?))
    }
}

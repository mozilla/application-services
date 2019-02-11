/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub use crate::http_client::{
    DeviceLocation as Location, DeviceType as Type, GetDeviceResponse as Device, PushSubscription,
};
use crate::{
    commands::{self, send_tab::SendTabPayload},
    errors::*,
    http_client::{
        CommandData, DeviceUpdateRequest, DeviceUpdateRequestBuilder, PendingCommand,
        UpdateDeviceResponse,
    },
    AccountEvent, FirefoxAccount,
};
use serde_derive::*;
use std::collections::HashMap;

impl FirefoxAccount {
    /// Fetches the list of devices from the current account including
    /// the current one.
    pub fn get_devices(&self) -> Result<Vec<Device>> {
        let refresh_token = self.get_refresh_token()?;
        self.client.devices(&self.state.config, &refresh_token)
    }

    pub fn get_current_device(&self) -> Result<Option<Device>> {
        Ok(self
            .get_devices()?
            .into_iter()
            .find(|d| d.is_current_device))
    }

    /// Initalizes our own device, most of the time this will be called right after logging-in
    /// for the first time.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn initialize_device(
        &mut self,
        name: &str,
        device_type: Type,
        capabilities: &[Capability],
    ) -> Result<()> {
        let mut commands = HashMap::new();
        for capability in capabilities {
            match capability {
                Capability::SendTab => {
                    let send_tab_command = self.generate_send_tab_command_data()?;
                    commands.insert(
                        commands::send_tab::COMMAND_NAME.to_owned(),
                        send_tab_command.to_owned(),
                    );
                    self.state.device_capabilities.insert(Capability::SendTab);
                }
            }
        }
        let update = DeviceUpdateRequestBuilder::new()
            .display_name(name)
            .device_type(&device_type)
            .available_commands(&commands)
            .build();
        self.update_device(update)?;
        Ok(())
    }

    /// Ensure that the capabilities requested earlier in initialize_device are A-OK.
    /// As the only capability is Send Tab now, its command is registered with the server.
    /// Don't forget to also call this if the Sync Keys change as they
    /// encrypt the Send Tab command data.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn ensure_capabilities(&mut self) -> Result<()> {
        for capability in self.state.device_capabilities.clone() {
            match capability {
                Capability::SendTab => {
                    let send_tab_command = self.generate_send_tab_command_data()?;
                    self.register_command(commands::send_tab::COMMAND_NAME, &send_tab_command)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn invoke_command(
        &self,
        command: &str,
        target: &Device,
        payload: &serde_json::Value,
    ) -> Result<()> {
        let refresh_token = self.get_refresh_token()?;
        self.client.invoke_command(
            &self.state.config,
            &refresh_token,
            command,
            &target.id,
            payload,
        )
    }

    /// Poll and parse any pending available command for our device.
    /// This should be called semi-regularly as the main method of
    /// commands delivery (push) can sometimes be unreliable on mobile devices.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn poll_device_commands(&mut self) -> Result<Vec<AccountEvent>> {
        let last_command_index = self.state.last_handled_command.unwrap_or(0);
        // We increment last_command_index by 1 because the server response includes the current index.
        self.fetch_and_parse_commands(last_command_index + 1, None)
    }

    /// Retrieve and parse a specific command designated by its index.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn fetch_device_command(&mut self, index: u64) -> Result<AccountEvent> {
        let mut account_events = self.fetch_and_parse_commands(index, Some(1))?;
        let account_event = account_events
            .pop()
            .ok_or_else(|| ErrorKind::IllegalState("Index fetch came out empty."))?;
        if !account_events.is_empty() {
            log::warn!("Index fetch resulted in more than 1 element");
        }
        Ok(account_event)
    }

    fn fetch_and_parse_commands(
        &mut self,
        index: u64,
        limit: Option<u64>,
    ) -> Result<Vec<AccountEvent>> {
        let refresh_token = self.get_refresh_token()?;
        let pending_commands =
            self.client
                .pending_commands(&self.state.config, refresh_token, index, limit)?;
        if pending_commands.messages.is_empty() {
            return Ok(Vec::new());
        }
        log::info!("Handling {} messages", pending_commands.messages.len());
        let account_events = self.parse_commands_messages(pending_commands.messages)?;
        self.state.last_handled_command = Some(pending_commands.index);
        Ok(account_events)
    }

    fn parse_commands_messages(&self, messages: Vec<PendingCommand>) -> Result<Vec<AccountEvent>> {
        let mut account_events: Vec<AccountEvent> = Vec::with_capacity(messages.len());
        let commands: Vec<_> = messages.into_iter().map(|m| m.data).collect();
        let devices = self.get_devices()?;
        for data in commands {
            match self.parse_command(data, &devices) {
                Ok((sender, tab)) => account_events.push(AccountEvent::TabReceived((sender, tab))),
                Err(e) => log::error!("Error while processing command: {}", e),
            };
        }
        Ok(account_events)
    }

    // Returns SendTabPayload for now because we only receive send-tab commands and
    // it's way easier, but should probably return AccountEvent or similar in the future.
    fn parse_command(
        &self,
        command_data: CommandData,
        devices: &[Device],
    ) -> Result<(Option<Device>, SendTabPayload)> {
        let sender = command_data
            .sender
            .and_then(|s| devices.iter().find(|i| i.id == s).cloned());
        match command_data.command.as_str() {
            commands::send_tab::COMMAND_NAME => {
                self.handle_send_tab_command(sender, command_data.payload)
            }
            _ => Err(ErrorKind::UnknownCommand(command_data.command).into()),
        }
    }

    pub fn set_device_name(&self, name: &str) -> Result<UpdateDeviceResponse> {
        let update = DeviceUpdateRequestBuilder::new().display_name(name).build();
        self.update_device(update)
    }

    pub fn clear_device_name(&self) -> Result<UpdateDeviceResponse> {
        let update = DeviceUpdateRequestBuilder::new()
            .clear_display_name()
            .build();
        self.update_device(update)
    }

    pub fn set_push_subscription(
        &self,
        push_subscription: &PushSubscription,
    ) -> Result<UpdateDeviceResponse> {
        let update = DeviceUpdateRequestBuilder::new()
            .push_subscription(&push_subscription)
            .build();
        self.update_device(update)
    }

    // TODO: this currently overwrites every other registered command
    // for the device because the server does not have a `PATCH commands`
    // endpoint yet.
    pub(crate) fn register_command(
        &self,
        command: &str,
        value: &str,
    ) -> Result<UpdateDeviceResponse> {
        let mut commands = HashMap::new();
        commands.insert(command.to_owned(), value.to_owned());
        let update = DeviceUpdateRequestBuilder::new()
            .available_commands(&commands)
            .build();
        self.update_device(update)
    }

    // TODO: this currently deletes every command registered for the device
    // because the server does not have a `PATCH commands` endpoint yet.
    #[allow(dead_code)]
    pub(crate) fn unregister_command(&self, _: &str) -> Result<UpdateDeviceResponse> {
        let commands = HashMap::new();
        let update = DeviceUpdateRequestBuilder::new()
            .available_commands(&commands)
            .build();
        self.update_device(update)
    }

    #[allow(dead_code)]
    pub(crate) fn clear_commands(&self) -> Result<UpdateDeviceResponse> {
        let update = DeviceUpdateRequestBuilder::new()
            .clear_available_commands()
            .build();
        self.update_device(update)
    }

    pub(crate) fn replace_device(
        &self,
        display_name: &str,
        device_type: &Type,
        push_subscription: &Option<PushSubscription>,
        commands: &HashMap<String, String>,
    ) -> Result<UpdateDeviceResponse> {
        let mut builder = DeviceUpdateRequestBuilder::new()
            .display_name(display_name)
            .device_type(device_type)
            .available_commands(commands);
        if let Some(push_subscription) = push_subscription {
            builder = builder.push_subscription(push_subscription)
        }
        self.update_device(builder.build())
    }

    fn update_device(&self, update: DeviceUpdateRequest<'_>) -> Result<UpdateDeviceResponse> {
        let refresh_token = self.get_refresh_token()?;
        self.client
            .update_device(&self.state.config, refresh_token, update)
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Capability {
    SendTab,
}

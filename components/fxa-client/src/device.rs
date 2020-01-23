/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub use crate::http_client::{
    DeviceLocation as Location, DeviceType as Type, GetDeviceResponse as Device, PushSubscription,
};
use crate::{
    commands,
    error::*,
    http_client::{
        CommandData, DeviceUpdateRequest, DeviceUpdateRequestBuilder, PendingCommand,
        UpdateDeviceResponse,
    },
    FirefoxAccount, IncomingDeviceCommand,
};
use serde_derive::*;
use std::collections::{HashMap, HashSet};

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

    /// Replaces the internal set of "tracked" device capabilities by re-registering
    /// new capabilities and returns a set of device commands to register with the
    /// server.
    fn register_capabilities(
        &mut self,
        capabilities: &[Capability],
    ) -> Result<HashMap<String, String>> {
        let mut capabilities_set = HashSet::new();
        let mut commands = HashMap::new();
        for capability in capabilities {
            match capability {
                Capability::SendTab => {
                    let send_tab_command = self.generate_send_tab_command_data()?;
                    commands.insert(
                        commands::send_tab::COMMAND_NAME.to_owned(),
                        send_tab_command.to_owned(),
                    );
                    capabilities_set.insert(Capability::SendTab);
                }
            }
        }
        self.state.device_capabilities = capabilities_set;
        Ok(commands)
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
        let commands = self.register_capabilities(capabilities)?;
        let update = DeviceUpdateRequestBuilder::new()
            .display_name(name)
            .device_type(&device_type)
            .available_commands(&commands)
            .build();
        let resp = self.update_device(update)?;
        self.state.current_device_id = Option::from(resp.id);
        Ok(())
    }

    /// Register a set of device capabilities against the current device.
    ///
    /// As the only capability is Send Tab now, its command is registered with the server.
    /// Don't forget to also call this if the Sync Keys change as they
    /// encrypt the Send Tab command data.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn ensure_capabilities(&mut self, capabilities: &[Capability]) -> Result<()> {
        let commands = self.register_capabilities(capabilities)?;
        let update = DeviceUpdateRequestBuilder::new()
            .available_commands(&commands)
            .build();
        let resp = self.update_device(update)?;
        self.state.current_device_id = Option::from(resp.id);
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
    pub fn poll_device_commands(&mut self) -> Result<Vec<IncomingDeviceCommand>> {
        let last_command_index = self.state.last_handled_command.unwrap_or(0);
        // We increment last_command_index by 1 because the server response includes the current index.
        self.fetch_and_parse_commands(last_command_index + 1, None)
    }

    /// Retrieve and parse a specific command designated by its index.
    ///
    /// **💾 This method alters the persisted account state.**
    pub fn fetch_device_command(&mut self, index: u64) -> Result<IncomingDeviceCommand> {
        let mut device_commands = self.fetch_and_parse_commands(index, Some(1))?;
        let device_command = device_commands
            .pop()
            .ok_or_else(|| ErrorKind::IllegalState("Index fetch came out empty."))?;
        if !device_commands.is_empty() {
            log::warn!("Index fetch resulted in more than 1 element");
        }
        Ok(device_command)
    }

    fn fetch_and_parse_commands(
        &mut self,
        index: u64,
        limit: Option<u64>,
    ) -> Result<Vec<IncomingDeviceCommand>> {
        let refresh_token = self.get_refresh_token()?;
        let pending_commands =
            self.client
                .pending_commands(&self.state.config, refresh_token, index, limit)?;
        if pending_commands.messages.is_empty() {
            return Ok(Vec::new());
        }
        log::info!("Handling {} messages", pending_commands.messages.len());
        let device_commands = self.parse_commands_messages(pending_commands.messages)?;
        self.state.last_handled_command = Some(pending_commands.index);
        Ok(device_commands)
    }

    fn parse_commands_messages(
        &self,
        messages: Vec<PendingCommand>,
    ) -> Result<Vec<IncomingDeviceCommand>> {
        let devices = self.get_devices()?;
        let parsed_commands = messages
            .into_iter()
            .filter_map(|msg| match self.parse_command(msg.data, &devices) {
                Ok(device_command) => Some(device_command),
                Err(e) => {
                    log::error!("Error while processing command: {}", e);
                    None
                }
            })
            .collect();
        Ok(parsed_commands)
    }

    fn parse_command(
        &self,
        command_data: CommandData,
        devices: &[Device],
    ) -> Result<IncomingDeviceCommand> {
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
    #[allow(dead_code)]
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

    /// Retrieve the current device id from state
    pub fn get_current_device_id(&mut self) -> Result<String> {
        match self.state.current_device_id {
            Some(ref device_id) => Ok(device_id.to_string()),
            None => Err(ErrorKind::NoCurrentDeviceId.into()),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Capability {
    SendTab,
}

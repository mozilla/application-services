/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

pub mod ffi;
use fxa_client::error::*; // TODO: Make our own errors
use fxa_client::{
    device::{Capability as DeviceCapability, Device, Type as DeviceType},
    scopes, Config as FxAConfig, FirefoxAccount, Profile,
};
// Include the `msg_types` module, which is generated from accounts_msg_types.proto.
pub mod msg_types {
    include!(concat!(env!("OUT_DIR"), "/msg_types.rs"));
}

// State machine events.
#[derive(Debug, Clone)]
enum Event {
    // External inputs.
    Init { state_json: Option<String> },
    CompletedAuthFlow { code: String, state: String },
    AuthenticationError,
    Logout,
    // State-machine generated events.
    AccountCreated,
    AccountRestored,
    RecoveredFromAuthenticationProblem,
    FailedToAuthenticate,
}

// State machine states.
#[derive(Debug, Clone, Copy)]
pub enum State {
    Start,
    NotAuthenticated,
    AuthenticationProblem,
    Authenticated,
}

pub struct FxAccountManager {
    remote_config: FxAConfig,
    state: State,
    account: FirefoxAccount,
    device_config: DeviceConfig,
    profile: Option<Profile>,
    devices: Vec<Device>,
    // app_scopes: Vec<String>, TODO
}

impl FxAccountManager {
    pub fn new(remote_config: FxAConfig, device_config: DeviceConfig) -> Self {
        let fxa_config = remote_config.clone();
        Self {
            remote_config,
            state: State::Start,
            account: FirefoxAccount::with_config(fxa_config),
            device_config,
            profile: None,
            devices: vec![],
        }
    }

    pub fn init(&mut self, state_json: Option<&str>) {
        self.process_event(Event::Init {
            state_json: state_json.map(|s| s.to_owned()),
        });
    }

    pub fn begin_oauth_flow(&mut self) -> Result<String> {
        // Can't really return values from state machine pumping, so here we go instead.
        self.account.begin_oauth_flow(&[scopes::PROFILE]) // TODO app_scopes
    }

    pub fn begin_pairing_flow(&mut self, pairing_url: &str) -> Result<String> {
        self.account
            .begin_pairing_flow(pairing_url, &[scopes::PROFILE]) // TODO app_scopes
    }

    pub fn finish_authentication_flow(&mut self, code: &str, state: &str) {
        self.process_event(Event::CompletedAuthFlow {
            code: code.to_owned(),
            state: state.to_owned(),
        });
    }

    pub fn get_profile(&mut self) -> Option<Profile> {
        self.profile.clone()
    }

    pub fn update_profile(&mut self) -> Option<Profile> {
        let new_profile = match self.account.get_profile(false) {
            Ok(p) => Some(p),
            Err(e) => match e.kind() {
                ErrorKind::RemoteError { code: 401, .. } => {
                    self.state = State::AuthenticationProblem;
                    None
                }
                _ => None,
            },
        };
        self.profile = new_profile;
        self.profile.clone()
    }

    // Called by Sync or for any third-party service that uses an OAuth token.
    pub fn on_authentication_error(&mut self) {
        self.process_event(Event::AuthenticationError)
    }

    pub fn logout(&mut self) {
        self.process_event(Event::Logout)
    }

    pub fn account_state(&self) -> State {
        self.state
    }

    pub fn export_persisted_state(&self) -> Result<String> {
        self.account.to_json()
    }

    pub fn update_devices(&mut self) -> DeviceConstellation {
        match self.account.get_devices() {
            Ok(devices) => {
                self.devices = devices;
            }
            Err(e) => match e.kind() {
                ErrorKind::RemoteError { code: 401, .. } => {
                    self.state = State::AuthenticationProblem;
                    self.devices = vec![];
                }
                _ => {}
            },
        };
        self.get_devices()
    }

    pub fn get_devices(&self) -> DeviceConstellation {
        let (mut own_devices, other_devices): (Vec<_>, Vec<_>) = self
            .devices
            .iter()
            .map(|d| d.clone())
            .partition(|d| d.is_current_device);
        let current_device = match own_devices.len() {
            0 => None,
            1 => Some(own_devices.remove(0)),
            _ => {
                log::error!("Found multiple own devices?!");
                Some(own_devices.remove(0))
            }
        };
        DeviceConstellation {
            current_device,
            other_devices,
        }
    }

    // Escape hatch used by the FFI crate. Do not use!
    pub fn get_account(&mut self) -> &mut FirefoxAccount {
        &mut self.account
    }

    /// State transition matrix.
    /// If state+event results in a transition, a state
    /// is returned.
    fn next_state(state: &State, event: &Event) -> Option<State> {
        match state {
            State::Start => match event {
                Event::Init { .. } => Some(State::Start),
                Event::AccountRestored => Some(State::Authenticated),
                Event::AccountCreated => Some(State::NotAuthenticated),
                _ => None,
            },
            State::NotAuthenticated => match event {
                Event::FailedToAuthenticate => Some(State::NotAuthenticated),
                Event::CompletedAuthFlow { .. } => Some(State::Authenticated),
                _ => None,
            },
            State::Authenticated => match event {
                Event::AuthenticationError => Some(State::AuthenticationProblem),
                Event::Logout => Some(State::NotAuthenticated),
                _ => None,
            },
            State::AuthenticationProblem => match event {
                Event::FailedToAuthenticate => Some(State::AuthenticationProblem),
                Event::RecoveredFromAuthenticationProblem => Some(State::Authenticated),
                Event::CompletedAuthFlow { .. } => Some(State::Authenticated),
                Event::Logout => Some(State::NotAuthenticated),
                _ => None,
            },
        }
    }

    fn state_actions(&mut self, via: &Event) -> Option<Event> {
        match self.state {
            State::Start => match via {
                Event::Init { state_json } => {
                    let saved_account = state_json.as_ref().and_then(|json| {
                        match FirefoxAccount::from_json(json) {
                            Ok(restored_account) => Some(restored_account),
                            Err(e) => {
                                log::error!(
                                    "Failed to load saved account: {}. Re-initializing...",
                                    e
                                );
                                None
                            }
                        }
                    });
                    if let Some(acct) = saved_account {
                        self.account = acct;
                        Some(Event::AccountRestored)
                    } else {
                        // Use the account provided in the constructor.
                        Some(Event::AccountCreated)
                    }
                }
                _ => None,
            },
            State::NotAuthenticated => match via {
                Event::Logout => {
                    self.account.disconnect();
                    self.account = FirefoxAccount::with_config(self.remote_config.clone());
                    None
                }
                Event::AccountCreated => {
                    // coolbeans.png
                    None
                }
                _ => None,
            },
            State::Authenticated => match via {
                Event::AccountRestored => {
                    log::info!("Ensuring device capabilities...");
                    if let Err(err) = self
                        .account
                        .ensure_capabilities(&self.device_config.capabilities)
                    {
                        log::warn!("Failed to ensure device capabilities: {}", err);
                    } else {
                        log::info!("Successfully ensured device capabilities.");
                    }
                    Some(Event::FailedToAuthenticate)
                }
                Event::CompletedAuthFlow { code, state } => {
                    log::info!("Completing OAuth flow...");
                    if let Err(err) = self.account.complete_oauth_flow(code, state) {
                        log::error!("Could not complete the OAuth flow: {}", err);
                    }
                    log::info!("Registering device...");
                    if let Err(err) = self.account.initialize_device(
                        &self.device_config.name,
                        self.device_config.r#type,
                        &self.device_config.capabilities,
                    ) {
                        log::warn!("Failed to register device: {}", err);
                    } else {
                        log::info!("Successfully registered device.");
                    }
                    None
                }
                Event::RecoveredFromAuthenticationProblem => {
                    log::info!("Registering device...");
                    if let Err(err) = self.account.initialize_device(
                        &self.device_config.name,
                        self.device_config.r#type,
                        &self.device_config.capabilities,
                    ) {
                        log::warn!("Failed to register device: {}", err);
                    } else {
                        log::info!("Successfully registered device.");
                    }
                    None
                }
                _ => None,
            },
            State::AuthenticationProblem => match via {
                Event::AuthenticationError => {
                    log::info!("Hit authentication problem. Trying to recover.");
                    self.account.clear_access_token_cache();
                    if self.account.get_access_token(scopes::PROFILE).is_ok() {
                        log::info!("Able to recover from an authentication problem.");
                        return Some(Event::RecoveredFromAuthenticationProblem);
                    }
                    None
                }
                _ => None,
            },
        }
    }

    // Pums the state machine until it settles.
    fn process_event(&mut self, input_event: Event) {
        let mut to_process = Some(input_event);
        while let Some(event) = to_process {
            let next_state = match Self::next_state(&self.state, &event) {
                Some(e) => e,
                None => {
                    log::warn!(
                        "Got invalid event {:#?} for state {:#?}.",
                        event,
                        self.state
                    );
                    return;
                }
            };
            log::info!(
                "Processing event {:#?} for state {:#?}. Next state is {:#?}",
                event,
                self.state,
                next_state
            );
            self.state = next_state;
            to_process = self.state_actions(&event);
            if to_process.is_some() {
                log::info!(
                    "Ran '{:#?}' side-effects for state {:#?}, got successive event {:#?}",
                    event,
                    self.state,
                    to_process
                );
            }
        }
    }
}

pub struct DeviceConfig {
    name: String,
    r#type: DeviceType,
    capabilities: Vec<DeviceCapability>,
}

pub struct DeviceConstellation {
    current_device: Option<Device>,
    other_devices: Vec<Device>,
}

impl DeviceConfig {
    pub fn new(name: &str, r#type: DeviceType, capabilities: Vec<DeviceCapability>) -> Self {
        Self {
            name: name.to_owned(),
            r#type,
            capabilities,
        }
    }
}

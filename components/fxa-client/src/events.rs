/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{AuthState, CallbackResult, DeviceList, FirefoxAccount, IncomingDeviceCommand};

impl FirefoxAccount {
    /// Register to receive FxaEvent notifications
    ///
    /// If there is EventListener already registered, it will be unregistered.  Pass in None to
    /// remove the current listener
    pub fn register_event_listener(&self, listener: Option<Box<dyn EventListener>>) {
        self.internal.lock().register_event_listener(listener);
    }
}

/// Implemented by a foreign callback interface to listen for FxA events.
/// Use `FirefoxAccount::register_event_listener()` to receive these events.
pub trait EventListener: Send + Sync {
    fn on_event(&self, event: FxaEvent) -> CallbackResult<()>;
}

#[derive(Debug)]
pub enum FxaEvent {
    /// The account's profile was updated
    ProfileUpdated,
    /// Account auth state changed
    AuthStateChanged { state: AuthState },
    /// The account itself was destroyed
    AccountDestroyed,
    /// An command from another device was received
    DeviceCommandIncoming { command: IncomingDeviceCommand },
    /// The list of devices was changed
    DeviceListChanged { device_list: DeviceList },
}

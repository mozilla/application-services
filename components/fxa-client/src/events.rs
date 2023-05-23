/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::FxaState;
use crate::Device;

/// FxA event handler trait
pub trait FxaEventHandler {
    fn on_event(&self, event: FxaEvent);
    fn on_command(&self, command: FxaCommand);
}

/// FxA event that the app may want to respond to
pub enum FxaEvent {
    /// The FxA client state changed
    StateChanged {
        state: FxaState,
    },
    /// The account itself was deleted
    AccountDeleted,
    /// Error when trying to migrate old account data
    AccountMigrationFailed {
        // TODO: what fields would be useful here
    },
    /// The account's profile was updated
    ProfileUpdated,
    /// The list of devices changed
    DevicesChanged {
        /// Device record for this client
        client_devices: Device,
        /// Device record for all other known clients
        other_devices: Vec<Device>,
    },
    // I don't think we need these:
    // DeviceConnected
    // DeviceDisconnected
    // DeviceCommandIncoming
    //
    // I also don't think we should implement the `SyncStatusObserver` functionality.  If needed,
    // we should add that to SyncManager
}

/// Received FxA command
pub enum FxaCommand {
    SendTab{
        title: String,
        url: String,
    }
}

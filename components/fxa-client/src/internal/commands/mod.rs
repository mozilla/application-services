/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub mod close_tabs;
mod keys;
pub mod send_tab;

pub use close_tabs::CloseTabsPayload;
pub use send_tab::SendTabPayload;

pub(crate) use keys::{
    decrypt_command, encrypt_command, get_public_keys, PrivateCommandKeys, PublicCommandKeys,
};

use super::device::Device;
use crate::{Error, Result};

// Currently public for use by example crates, but should be made private eventually.
#[derive(Clone, Debug)]
pub enum IncomingDeviceCommand {
    TabReceived {
        sender: Option<Device>,
        payload: SendTabPayload,
    },
    TabsClosed {
        sender: Option<Device>,
        payload: CloseTabsPayload,
    },
}

impl TryFrom<IncomingDeviceCommand> for crate::IncomingDeviceCommand {
    type Error = Error;
    fn try_from(cmd: IncomingDeviceCommand) -> Result<Self> {
        Ok(match cmd {
            IncomingDeviceCommand::TabReceived { sender, payload } => {
                crate::IncomingDeviceCommand::TabReceived {
                    sender: sender.map(crate::Device::try_from).transpose()?,
                    payload: payload.into(),
                }
            }
            IncomingDeviceCommand::TabsClosed { sender, payload } => {
                crate::IncomingDeviceCommand::TabsClosed {
                    sender: sender.map(crate::Device::try_from).transpose()?,
                    payload: payload.into(),
                }
            }
        })
    }
}

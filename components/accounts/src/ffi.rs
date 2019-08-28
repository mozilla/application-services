/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{msg_types, DeviceConstellation, State};
use ffi_support::{implement_into_ffi_by_delegation, implement_into_ffi_by_protobuf};

impl From<State> for msg_types::AccountState {
    fn from(s: State) -> Self {
        let state = match s {
            State::Start => msg_types::account_state::State::Start,
            State::NotAuthenticated => msg_types::account_state::State::NotAuthenticated,
            State::AuthenticationProblem => msg_types::account_state::State::AuthenticationProblem,
            State::Authenticated => msg_types::account_state::State::Authenticated,
        };
        Self {
            state: state as i32,
        }
    }
}

impl From<DeviceConstellation> for fxa_client::msg_types::DeviceConstellation {
    fn from(d: DeviceConstellation) -> Self {
        let devices = d
            .other_devices
            .into_iter()
            .map(|device| device.into())
            .collect();
        let other_devices = fxa_client::msg_types::Devices { devices };
        Self {
            current_device: d.current_device.map(|d| d.into()),
            other_devices,
        }
    }
}

implement_into_ffi_by_protobuf!(msg_types::AccountState);
implement_into_ffi_by_delegation!(State, msg_types::AccountState);
implement_into_ffi_by_delegation!(
    DeviceConstellation,
    fxa_client::msg_types::DeviceConstellation
);

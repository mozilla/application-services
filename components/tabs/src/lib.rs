/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

#[macro_use]
pub mod error;
mod schema;
mod storage;
mod store;
mod sync;

uniffi_macros::include_scaffolding!("tabs");

pub use crate::storage::{ClientRemoteTabs, RemoteTabRecord, TabsDeviceType};
pub use crate::store::TabsStore;
use error::TabsError;
use sync15::DeviceType;

#[cfg(feature = "full-sync")]
pub use crate::sync::engine::get_registered_sync_engine;
#[cfg(feature = "full-sync")]
pub use crate::sync::engine::TabsEngine;

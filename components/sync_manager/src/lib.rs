/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

pub mod error;
pub mod manager;
mod types;

pub use sync15::DeviceType;

pub use error::{Result, SyncManagerError};
pub use types::*;

use manager::SyncManager;
use parking_lot::Mutex;

lazy_static::lazy_static! {
    static ref MANAGER: Mutex<SyncManager> = Mutex::new(SyncManager::new());
}

pub(crate) fn wipe(engine: &str) -> Result<()> {
    let manager = MANAGER.lock();
    manager.wipe(engine)
}

pub(crate) fn reset(engine: &str) -> Result<()> {
    let manager = MANAGER.lock();
    manager.reset(engine)
}

pub(crate) fn reset_all() -> Result<()> {
    let manager = MANAGER.lock();
    manager.reset_all()
}

uniffi::include_scaffolding!("syncmanager");

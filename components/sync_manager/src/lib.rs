/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

pub mod error;
mod ffi;
pub mod manager;

pub use error::{Error, ErrorKind, Result};

pub mod msg_types {
    include!("mozilla.appservices.syncmanager.protobuf.rs");
}

use manager::SyncManager;
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref MANAGER: Mutex<SyncManager> = Mutex::new(SyncManager::new());
}

pub fn disconnect() {
    let mut manager = MANAGER.lock().unwrap();
    manager.disconnect();
}

pub fn wipe(engine: &str) -> Result<()> {
    let mut manager = MANAGER.lock().unwrap();
    manager.wipe(engine)
}

pub fn reset(engine: &str) -> Result<()> {
    let mut manager = MANAGER.lock().unwrap();
    manager.reset(engine)
}

pub fn reset_all() -> Result<()> {
    let mut manager = MANAGER.lock().unwrap();
    manager.reset_all()
}

pub fn sync(params: msg_types::SyncParams) -> Result<msg_types::SyncResult> {
    let mut manager = MANAGER.lock().unwrap();
    manager.sync(params)
}

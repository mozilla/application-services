/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

pub mod error;
mod ffi;
mod manager;

pub use error::{Error, ErrorKind, Result};

pub mod msg_types {
    include!(concat!(env!("OUT_DIR"), "/msg_types.rs"));
}

#[cfg(feature = "logins")]
use logins::PasswordEngine;
use manager::SyncManager;
#[cfg(feature = "places")]
use places::PlacesApi;
#[cfg(any(feature = "places", feature = "logins"))]
use std::sync::Arc;
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref MANAGER: Mutex<SyncManager> = Mutex::new(SyncManager::new());
}

#[cfg(feature = "places")]
pub fn set_places(places: Arc<PlacesApi>) {
    let mut manager = MANAGER.lock().unwrap();
    manager.set_places(places);
}

#[cfg(feature = "logins")]
pub fn set_logins(places: Arc<Mutex<PasswordEngine>>) {
    let mut manager = MANAGER.lock().unwrap();
    manager.set_logins(places);
}

pub fn disconnect() {
    let mut manager = MANAGER.lock().unwrap();
    manager.disconnect();
}

pub fn sync(params: msg_types::SyncParams) -> Result<msg_types::SyncResult> {
    let mut manager = MANAGER.lock().unwrap();
    // TODO: translate the protobuf message into something nicer to work with in
    // Rust.
    manager.sync(params)
}

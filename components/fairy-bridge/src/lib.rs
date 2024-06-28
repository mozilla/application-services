/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

mod backend;
mod backends;
mod error;
pub mod headers;
mod request;

pub use backend::*;
// allow(unused) because there will be nothing to export if the `backend-reqwest` feature is
// disabled.
#[allow(unused)]
pub use backends::*;
pub use error::*;
pub use request::*;

static REGISTERED_BACKEND: OnceLock<Arc<dyn Backend>> = OnceLock::new();

#[derive(Default, uniffi::Record)]
pub struct Response {
    pub url: String,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

uniffi::setup_scaffolding!();

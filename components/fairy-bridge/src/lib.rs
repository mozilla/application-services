/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use url::Url;

mod backend;
mod backends;
mod error;
pub mod headers;
mod request;

pub use backend::*;
// allow(unused) because there will be nothing to export if the `backend-hyper` feature is
// disabled.
#[allow(unused)]
pub use backends::*;
pub use error::*;
pub use request::*;

static REGISTERED_BACKEND: OnceLock<Arc<dyn Backend>> = OnceLock::new();

#[derive(uniffi::Record, Clone)]
pub struct Response {
    pub url: Url,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Default)]
pub struct Client {
    settings: RequestSettings,
}

/// Fairy Bridge client
///
/// This is mostly for convenience.
/// It stores settings data and uses it for all requests.
impl Client {
    pub fn new(settings: RequestSettings) -> Self {
        Self { settings }
    }

    pub fn request(&self, method: Method, url: Url) -> Request {
        Request::new(self.settings.clone(), method, url)
    }

    pub fn get(&self, url: Url) -> Request {
        self.request(Method::Get, url)
    }

    pub fn patch(&self, url: Url) -> Request {
        self.request(Method::Patch, url)
    }

    pub fn post(&self, url: Url) -> Request {
        self.request(Method::Post, url)
    }

    pub fn put(&self, url: Url) -> Request {
        self.request(Method::Put, url)
    }

    pub fn delete(&self, url: Url) -> Request {
        self.request(Method::Delete, url)
    }
}

uniffi::setup_scaffolding!();

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This is where the persistence logic might go.
//! An idea for what to use here might be [RKV](https://github.com/mozilla/rkv)
//! And that's what's used on this prototype,
//! Either ways, the solution implemented should work regardless of the platform
//! on the other side of the FFI. This means that this module might require the FFI to allow consumers
//! To pass in a path to a database, or somewhere in the file system that the state will be persisted

use anyhow::Result;
use std::path::Path;

pub struct Database {}

impl Database {
    #[allow(unused)]
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        unimplemented!();
    }

    #[allow(unused)]
    pub fn get<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(
        &self,
        key: &str,
    ) -> Result<Option<T>> {
        unimplemented!();
    }

    #[allow(unused)]
    pub fn put<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(
        &self,
        key: &str,
        persisted_data: &T,
    ) -> Result<()> {
        unimplemented!();
    }
}

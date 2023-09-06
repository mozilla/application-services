/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::error::Result;
use crate::stateful::client::{Experiment, SettingsClient};

/// This is a client for use when no server is provided.
/// Its primary use is for non-Mozilla forks of apps that are not using their
/// own server infrastructure.
pub struct NullClient;

impl NullClient {
    pub fn new() -> Self {
        NullClient
    }
}

impl SettingsClient for NullClient {
    fn get_experiments_metadata(&self) -> Result<String> {
        unimplemented!();
    }
    fn fetch_experiments(&self) -> Result<Vec<Experiment>> {
        Ok(Default::default())
    }
}

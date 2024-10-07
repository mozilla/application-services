/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! This is a simple HTTP client that uses viaduct to retrieve experiment data from the server.
//! Currently configured to use Kinto and the old schema, although that would change once we start
//! working on the real Nimbus schema.
//!
//! In the future we might replace this with a more fully-feature Remote Settings client, such as:
//!
//!   https://github.com/mozilla-services/remote-settings-client
//!   Issue: https://github.com/mozilla/application-services/issues/3475
//!
//! But the simple subset implemented here meets our needs for now.

use crate::error::Result;
use crate::schema::parse_experiments;
use crate::stateful::client::{Experiment, SettingsClient};
use remote_settings::RemoteSettings;

impl SettingsClient for RemoteSettings {
    fn get_experiments_metadata(&self) -> Result<String> {
        unimplemented!();
    }

    fn fetch_experiments(&self) -> Result<Vec<Experiment>> {
        let resp = self.get_records_raw()?;
        parse_experiments(&resp.text())
    }
}

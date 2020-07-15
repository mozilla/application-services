/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub use crate::http_client::ProfileResponse as Profile;
use crate::{error::*, FirefoxAccount};

impl FirefoxAccount {
    pub fn get_ecosystem_anon_id(&mut self) -> Result<Option<String>> {
        let profile = self.get_profile(false)?;
        Ok(profile.ecosystem_anon_id)
    }
}

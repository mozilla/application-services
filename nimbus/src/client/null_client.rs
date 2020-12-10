/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::error::Result;
use crate::{Experiment, SettingsClient};

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
    fn get_experiments(&mut self) -> Result<Vec<Experiment>> {
        Ok(vec![])
    }
}

#[cfg(feature = "rkv-safe-mode")]
#[test]
fn test_null_client() -> Result<()> {
    use crate::NimbusClient;
    use tempdir::TempDir;

    let _ = env_logger::try_init();

    let tmp_dir = TempDir::new("test_null_client-test_null")?;

    let aru = Default::default();
    let client = NimbusClient::new(Default::default(), tmp_dir.path(), None, aru)?;

    let experiments = client.get_all_experiments()?;
    assert_eq!(experiments.len(), 0);
    Ok(())
}

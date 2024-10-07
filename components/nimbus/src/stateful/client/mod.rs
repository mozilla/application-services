/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

mod fs_client;
pub(crate) mod http_client;
pub(crate) mod null_client;
use crate::error::{NimbusError, Result};
use crate::Experiment;
use fs_client::FileSystemClient;
use null_client::NullClient;
use remote_settings::RemoteSettings;
use remote_settings::RemoteSettingsConfig;

pub(crate) fn create_client(
    config: Option<RemoteSettingsConfig>,
) -> Result<Box<dyn SettingsClient + Send>> {
    Ok(match config {
        Some(config) => {
            assert!(config.server_url.is_none());
            let Some(remote_settings_server) = config.server.as_ref() else {
                return Ok(Box::new(RemoteSettings::new(config)?));
            };
            let url = remote_settings_server.url()?;
            if url.scheme() == "file" {
                // Everything in `config` other than the url/path is ignored for the
                // file-system - we could insist on a sub-directory, but that doesn't
                // seem valuable for the use-cases we care about here.
                let path = match url.to_file_path() {
                    Ok(path) => path,
                    _ => return Err(NimbusError::InvalidPath(url.into())),
                };
                Box::new(FileSystemClient::new(path)?)
            } else {
                Box::new(RemoteSettings::new(config)?)
            }
        }
        // If no server is provided, then we still want Nimbus to work, but serving
        // an empty list of experiments.
        None => Box::new(NullClient::new()),
    })
}

// The trait used to fetch experiments.
pub(crate) trait SettingsClient {
    #[allow(dead_code)]
    fn get_experiments_metadata(&self) -> Result<String>;
    fn fetch_experiments(&self) -> Result<Vec<Experiment>>;
}

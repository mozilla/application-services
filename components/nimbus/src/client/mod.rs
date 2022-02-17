// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod fs_client;
pub(crate) mod http_client;
mod null_client;
use crate::error::{NimbusError, Result};
use crate::Experiment;
use crate::RemoteSettingsConfig;
use fs_client::FileSystemClient;
use http_client::Client;
use null_client::NullClient;
use url::Url;

pub use http_client::parse_experiments;

pub(crate) fn create_client(
    config: Option<RemoteSettingsConfig>,
) -> Result<Box<dyn SettingsClient + Send>> {
    Ok(match config {
        Some(config) => {
            // XXX - double-parsing the URL here if it's not a file:// URL - ideally
            // config would already be holding a Url and we wouldn't parse here at all.
            let url = Url::parse(&config.server_url)?;
            if url.scheme() == "file" {
                // Everything in `config` other than the url/path is ignored for the
                // file-system - we could insist on a sub-directory, but that doesn't
                // seem valuable for the use-cases we care about here.
                let path = match url.to_file_path() {
                    Ok(path) => path,
                    _ => return Err(NimbusError::InvalidPath(config.server_url)),
                };
                Box::new(FileSystemClient::new(path)?)
            } else {
                Box::new(Client::new(config)?)
            }
        }
        // If no server is provided, then we still want Nimbus to work, but serving
        // an empty list of experiments.
        None => Box::new(NullClient::new()),
    })
}

// The trait used to fetch experiments.
pub(crate) trait SettingsClient {
    fn get_experiments_metadata(&self) -> Result<String>;
    fn fetch_experiments(&self) -> Result<Vec<Experiment>>;
}

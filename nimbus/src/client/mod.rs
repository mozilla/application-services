// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod fs_client;
mod http_client;
use crate::error::{Error, Result};
use crate::Experiment;
use crate::RemoteSettingsConfig;
use fs_client::FileSystemClient;
use http_client::Client;
use url::Url;

pub(crate) fn create_client(
    config: RemoteSettingsConfig,
) -> Result<Box<dyn SettingsClient + Send>> {
    // XXX - double-parsing the URL here if it's not a file:// URL - ideally
    // config would already be holding a Url and we wouldn't parse here at all.
    let url = Url::parse(&config.server_url)?;
    Ok(if url.scheme() == "file" {
        // Everything in `config` other than the url/path is ignored for the
        // file-system - we could insist on a sub-directory, but that doesn't
        // seem valuable for the use-cases we care about here.
        let path = match url.to_file_path() {
            Ok(path) => path,
            _ => return Err(Error::InvalidPath(config.server_url)),
        };
        Box::new(FileSystemClient::new(path)?)
    } else {
        Box::new(Client::new(config)?)
    })
}

// The trait used to fetch experiments.
pub(crate) trait SettingsClient {
    fn get_experiments_metadata(&self) -> Result<String>;
    fn get_experiments(&self) -> Result<Vec<Experiment>>;
}

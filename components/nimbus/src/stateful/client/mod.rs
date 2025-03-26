/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

mod fs_client;
pub(crate) mod http_client;
pub(crate) mod null_client;

use std::sync::Arc;

use crate::error::{NimbusError, Result};
use crate::Experiment;
use fs_client::FileSystemClient;
use null_client::NullClient;
use remote_settings::RemoteSettingsService;

pub(crate) fn create_client(
    collection_name: Option<String>,
    rs_service: Option<Arc<RemoteSettingsService>>,
) -> Result<Box<dyn SettingsClient + Send>> {
    Ok(match rs_service {
        Some(rs_service) => {
            let url = rs_service.client_url(); // let call this the server url
            match url.scheme() {
                "file" => {
                    // Everything in `config` other than the url/path is ignored for the
                    // file-system - we could insist on a sub-directory, but that doesn't
                    // seem valuable for the use-cases we care about here.
                    let path = match url.to_file_path() {
                        Ok(path) => path,
                        _ => return Err(NimbusError::InvalidPath(url.into())),
                    };
                    Box::new(FileSystemClient::new(path)?)
                }
                _ => match collection_name {
                    Some(collection_name) => Box::new(rs_service.make_client(collection_name)?),
                    _ => todo!(),
                },
            }
        }
        _ => Box::new(NullClient::new()),
    })
}

// The trait used to fetch experiments.
pub(crate) trait SettingsClient {
    #[allow(dead_code)]
    fn get_experiments_metadata(&self) -> Result<String>;
    fn fetch_experiments(&self) -> Result<Vec<Experiment>>;
}

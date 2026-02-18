/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

mod fs_client;
pub(crate) mod http_client;
pub(crate) mod null_client;

use std::sync::Arc;

use remote_settings::RemoteSettingsService;
use url::Url;

use crate::Experiment;
use crate::error::{NimbusError, Result};
use crate::stateful::client::fs_client::FileSystemClient;
use crate::stateful::client::null_client::NullClient;

pub struct NimbusServerSettings {
    pub rs_service: Arc<RemoteSettingsService>,
    pub collection_name: String,
}

pub(crate) fn create_client(
    rs_info: Option<NimbusServerSettings>,
) -> Result<Box<dyn SettingsClient + Send>> {
    Ok(match rs_info {
        Some(NimbusServerSettings {
            rs_service,
            collection_name,
        }) => {
            let url = Url::parse(&rs_service.client_url())?; // server url
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
                _ => Box::new(rs_service.make_client(collection_name)),
            }
        }
        None => Box::new(NullClient::new()),
    })
}

// The trait used to fetch experiments.
pub(crate) trait SettingsClient {
    #[allow(dead_code)]
    fn get_experiments_metadata(&self) -> Result<String>;
    fn fetch_experiments(&self) -> Result<Vec<Experiment>>;
}

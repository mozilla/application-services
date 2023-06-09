// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{
    config,
    value_utils::{self, CliUtils},
};
use anyhow::{bail, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExperimentListSource {
    FromRemoteSettings { endpoint: String, is_preview: bool },
    FromFile { file: PathBuf },
}

impl ExperimentListSource {
    pub(crate) fn try_from_pair(server: &str, preview: &str) -> Result<Self> {
        let is_preview = preview == "preview";

        let endpoint = match server {
            "" | "release" | "production" | "prod" => config::rs_production_server(),
            "stage" => config::rs_stage_server(),
            _ => bail!("Only stage or release currently supported"),
        };

        Ok(Self::FromRemoteSettings {
            endpoint,
            is_preview,
        })
    }
}

impl TryFrom<&str> for ExperimentListSource {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        let tokens: Vec<&str> = value.splitn(3, '/').collect();
        let tokens = tokens.as_slice();
        Ok(match tokens {
            [""] => Self::try_from_pair("", "")?,
            ["preview"] => Self::try_from_pair("", "preview")?,
            [server] => Self::try_from_pair(server, "")?,
            [server, "preview"] => Self::try_from_pair(server, "preview")?,
            _ => bail!(format!("Can't unpack '{}' into an experiment; try preview, release, stage, or stage/preview", value)),
        })
    }
}

impl TryFrom<&Path> for ExperimentListSource {
    type Error = anyhow::Error;

    fn try_from(value: &Path) -> Result<Self> {
        Ok(Self::FromFile {
            file: value.to_path_buf(),
        })
    }
}

// Get the experiment list

impl TryFrom<&ExperimentListSource> for Value {
    type Error = anyhow::Error;

    fn try_from(value: &ExperimentListSource) -> Result<Value> {
        Ok(match value {
            ExperimentListSource::FromRemoteSettings {
                endpoint,
                is_preview,
            } => {
                use remote_settings::{Client, RemoteSettingsConfig};
                viaduct_reqwest::use_reqwest_backend();
                let collection_name = if *is_preview {
                    "nimbus-preview".to_string()
                } else {
                    "nimbus-mobile-experiments".to_string()
                };
                let config = RemoteSettingsConfig {
                    server_url: Some(endpoint.clone()),
                    bucket_name: None,
                    collection_name,
                };
                let client = Client::new(config)?;

                let response = client.get_records_raw()?;
                response.json::<Value>()?
            }
            ExperimentListSource::FromFile { file } => {
                let v: Value = value_utils::read_from_file(file)?;
                if v.is_array() {
                    serde_json::json!({ "data": v })
                } else if v.get_array("data").is_ok() {
                    v
                } else if v.get_array("branches").is_ok() {
                    serde_json::json!({ "data": [v] })
                } else {
                    bail!(
                        "An unrecognized experiments JSON file: {}",
                        file.as_path().to_str().unwrap_or_default()
                    );
                }
            }
        })
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_experiment_list_from_pair() -> Result<()> {
        let release = config::rs_production_server();
        let stage = config::rs_stage_server();
        assert_eq!(
            ExperimentListSource::try_from_pair("", "")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_pair("", "preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: true
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_pair("release", "")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_pair("release", "preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: true
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_pair("stage", "")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: stage.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_pair("stage", "preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: stage,
                is_preview: true
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_pair("release", "preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release,
                is_preview: true
            }
        );

        assert!(ExperimentListSource::try_from_pair("not-real", "preview").is_err());

        Ok(())
    }

    #[test]
    fn test_experiment_list_from_str() -> Result<()> {
        let release = config::rs_production_server();
        let stage = config::rs_stage_server();
        assert_eq!(
            ExperimentListSource::try_from("")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from("release")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from("stage")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: stage.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from("preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: true
            }
        );
        assert_eq!(
            ExperimentListSource::try_from("release/preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release,
                is_preview: true
            }
        );
        assert_eq!(
            ExperimentListSource::try_from("stage/preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: stage,
                is_preview: true
            }
        );

        assert!(ExperimentListSource::try_from("not-real/preview").is_err());
        assert!(ExperimentListSource::try_from("release/not-real").is_err());

        Ok(())
    }
}

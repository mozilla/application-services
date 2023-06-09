// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{
    cli::{Cli, CliCommand, ExperimentListArgs},
    config,
    value_utils::{self, CliUtils},
    USER_AGENT,
};
use anyhow::{bail, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExperimentListSource {
    FromApiV6 { endpoint: String },
    FromRemoteSettings { endpoint: String, is_preview: bool },
    FromFile { file: PathBuf },
}

impl ExperimentListSource {
    fn try_from_slug<'a>(
        slug: &'a str,
        production: &'a str,
        stage: &'a str,
    ) -> Result<(&'a str, bool)> {
        let (is_production, is_preview) = decode_list_slug(slug)?;

        let endpoint = if is_production { production } else { stage };

        Ok((endpoint, is_preview))
    }

    pub(crate) fn try_from_rs(value: &str) -> Result<Self> {
        let p = config::rs_production_server();
        let s = config::rs_stage_server();
        let (endpoint, is_preview) = Self::try_from_slug(value, &p, &s)?;
        Ok(Self::FromRemoteSettings {
            endpoint: endpoint.to_string(),
            is_preview,
        })
    }

    pub(crate) fn try_from_api(value: &str) -> Result<Self> {
        let p = config::api_v6_production_server();
        let s = config::api_v6_stage_server();
        let (endpoint, _) = Self::try_from_slug(value, &p, &s)?;
        Ok(Self::FromApiV6 {
            endpoint: endpoint.to_string(),
        })
    }
}

// Returns (is_production, is_preview)
pub(crate) fn decode_list_slug(slug: &str) -> Result<(bool, bool)> {
    let tokens: Vec<&str> = slug.splitn(3, '/').collect();

    Ok(match tokens.as_slice() {
        [""] => (true, false),
        ["preview"] => (true, true),
        [server] => (is_production_server(server)?, false),
        [server, preview] => (
            is_production_server(server)?,
            is_preview_collection(preview)?,
        ),
        _ => bail!(format!(
            "Can't unpack '{slug}' into an experiment; try stage/SLUG, or SLUG"
        )),
    })
}

fn is_production_server(slug: &str) -> Result<bool> {
    Ok(match slug {
        "production" | "release" | "prod" | "" => true,
        "stage" | "staging" => false,
        _ => bail!(format!(
            "Cannot translate '{slug}' into production or stage"
        )),
    })
}

fn is_preview_collection(slug: &str) -> Result<bool> {
    Ok(match slug {
        "preview" => true,
        "" => false,
        _ => bail!(format!(
            "Cannot translate '{slug}' into preview or release collection"
        )),
    })
}

impl TryFrom<&Cli> for ExperimentListSource {
    type Error = anyhow::Error;

    fn try_from(value: &Cli) -> Result<Self> {
        Ok(match &value.command {
            CliCommand::FetchList { list, .. } | CliCommand::List { list } => {
                ExperimentListSource::try_from(list)?
            }
            _ => unreachable!(),
        })
    }
}

impl TryFrom<&ExperimentListArgs> for ExperimentListSource {
    type Error = anyhow::Error;

    fn try_from(value: &ExperimentListArgs) -> Result<Self> {
        Ok(match value {
            ExperimentListArgs {
                server,
                file: Some(file),
                ..
            } => {
                if !server.is_empty() {
                    bail!("Cannot load a list from a file AND a server")
                } else {
                    Self::FromFile { file: file.clone() }
                }
            }
            ExperimentListArgs {
                server: s,
                file: None,
                use_api,
            } => {
                if *use_api {
                    Self::try_from_api(s)?
                } else {
                    Self::try_from_rs(s)?
                }
            }
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
            ExperimentListSource::FromApiV6 { endpoint } => {
                let url = format!("{endpoint}/api/v6/experiments/");

                let req = reqwest::blocking::Client::builder()
                    .user_agent(USER_AGENT)
                    .gzip(true)
                    .build()?
                    .get(url);

                let resp = req.send()?;
                let data: Value = resp.json()?;

                fn start_date(v: &Value) -> &str {
                    let later = "9999-99-99";
                    match v.get("startDate") {
                        Some(v) => v.as_str().unwrap_or(later),
                        _ => later,
                    }
                }

                let data = match data {
                    Value::Array(mut array) => {
                        array.sort_by(|p, q| {
                            let p_time = start_date(p);
                            let q_time = start_date(q);
                            p_time.cmp(q_time)
                        });
                        Value::Array(array)
                    }
                    _ => data,
                };
                serde_json::json!({ "data": data })
            }
        })
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_experiment_list_from_rs() -> Result<()> {
        let release = config::rs_production_server();
        let stage = config::rs_stage_server();
        assert_eq!(
            ExperimentListSource::try_from_rs("")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_rs("preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: true
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_rs("release")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_rs("release/preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: true
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_rs("stage")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: stage.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_rs("stage/preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: stage,
                is_preview: true
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_rs("release/preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release,
                is_preview: true
            }
        );

        assert!(ExperimentListSource::try_from_rs("not-real/preview").is_err());
        assert!(ExperimentListSource::try_from_rs("release/not-real").is_err());

        Ok(())
    }
}

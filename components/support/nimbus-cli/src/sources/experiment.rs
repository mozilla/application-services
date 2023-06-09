// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{bail, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::{
    cli::{Cli, CliCommand, ExperimentArgs},
    config, feature_utils, value_utils, NimbusApp,
};

static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);
use super::{experiment_list::decode_list_slug, ExperimentListSource};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExperimentSource {
    FromList {
        slug: String,
        list: ExperimentListSource,
    },
    FromFeatureFiles {
        app: NimbusApp,
        feature_id: String,
        files: Vec<PathBuf>,
    },
    FromApiV6 {
        slug: String,
        endpoint: String,
    },
}

// Create ExperimentSources from &str and Cli.

impl ExperimentSource {
    fn try_from_slug<'a>(
        value: &'a str,
        production: &'a str,
        stage: &'a str,
    ) -> Result<(&'a str, &'a str, bool)> {
        let tokens: Vec<&str> = value.splitn(3, '/').collect();

        let (is_production, is_preview) = match tokens.as_slice() {
            [_] => decode_list_slug("")?,
            [first, _] => decode_list_slug(first)?,
            [first, second, _] => decode_list_slug(&format!("{first}/{second}"))?,
            _ => unreachable!(),
        };

        let endpoint = if is_production { production } else { stage };

        Ok(match tokens.last() {
            Some(slug) => (slug, endpoint, is_preview),
            _ => bail!(format!(
                "Can't unpack '{value}' into an experiment; try stage/SLUG, or SLUG"
            )),
        })
    }

    fn try_from_rs(value: &str) -> Result<Self> {
        let p = config::rs_production_server();
        let s = config::rs_stage_server();
        let (slug, endpoint, is_preview) = Self::try_from_slug(value, &p, &s)?;
        Ok(Self::FromList {
            slug: slug.to_string(),
            list: ExperimentListSource::FromRemoteSettings {
                endpoint: endpoint.to_string(),
                is_preview,
            },
        })
    }

    fn try_from_api(value: &str) -> Result<Self> {
        let p = config::api_v6_production_server();
        let s = config::api_v6_stage_server();
        let (slug, endpoint, _) = Self::try_from_slug(value, &p, &s)?;
        Ok(Self::FromApiV6 {
            slug: slug.to_string(),
            endpoint: endpoint.to_string(),
        })
    }

    pub(crate) fn try_from_file(file: &Path, slug: &str) -> Result<Self> {
        Ok(ExperimentSource::FromList {
            slug: slug.to_string(),
            list: file.try_into()?,
        })
    }
}

impl TryFrom<&ExperimentArgs> for ExperimentSource {
    type Error = anyhow::Error;

    fn try_from(value: &ExperimentArgs) -> Result<Self> {
        let experiment = &value.experiment;
        Ok(match &value.file {
            Some(file) => Self::try_from_file(file, experiment)?,
            _ if value.use_rs => Self::try_from_rs(experiment)?,
            _ => Self::try_from_api(experiment.as_str())?,
        })
    }
}

impl TryFrom<&Cli> for ExperimentSource {
    type Error = anyhow::Error;

    fn try_from(value: &Cli) -> Result<Self> {
        Ok(match &value.command {
            CliCommand::Validate { experiment, .. } | CliCommand::Enroll { experiment, .. } => {
                experiment.try_into()?
            }
            CliCommand::TestFeature {
                feature_id, files, ..
            } => Self::FromFeatureFiles {
                app: value.into(),
                feature_id: feature_id.clone(),
                files: files.clone(),
            },
            _ => unreachable!("Cli Arg not supporting getting an experiment source"),
        })
    }
}

// Get the experiment itself from the experiment source.

impl TryFrom<&ExperimentSource> for Value {
    type Error = anyhow::Error;

    fn try_from(value: &ExperimentSource) -> Result<Value> {
        Ok(match value {
            ExperimentSource::FromList { slug, list } => {
                let value = Value::try_from(list)?;
                value_utils::try_find_experiment(&value, slug)?
            }
            ExperimentSource::FromApiV6 { slug, endpoint } => {
                let url = format!("{endpoint}/api/v6/experiments/{slug}/");
                let req = reqwest::blocking::Client::builder()
                    .user_agent(USER_AGENT)
                    .gzip(true)
                    .build()?
                    .get(url);

                req.send()?.json()?
            }
            ExperimentSource::FromFeatureFiles {
                app,
                feature_id,
                files,
            } => feature_utils::create_experiment(app, feature_id, files)?,
        })
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    #[test]
    fn test_experiment_source_from_rs() -> Result<()> {
        let release = ExperimentListSource::try_from_rs("")?;
        let stage = ExperimentListSource::try_from_rs("stage")?;
        let release_preview = ExperimentListSource::try_from_rs("preview")?;
        let stage_preview = ExperimentListSource::try_from_rs("stage/preview")?;
        let slug = "my-slug".to_string();
        assert_eq!(
            ExperimentSource::try_from_rs("my-slug")?,
            ExperimentSource::FromList {
                list: release.clone(),
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from_rs("release/my-slug")?,
            ExperimentSource::FromList {
                list: release,
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from_rs("stage/my-slug")?,
            ExperimentSource::FromList {
                list: stage,
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from_rs("preview/my-slug")?,
            ExperimentSource::FromList {
                list: release_preview.clone(),
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from_rs("release/preview/my-slug")?,
            ExperimentSource::FromList {
                list: release_preview,
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from_rs("stage/preview/my-slug")?,
            ExperimentSource::FromList {
                list: stage_preview,
                slug
            }
        );

        assert!(ExperimentSource::try_from_rs("not-real/preview/my-slug").is_err());
        assert!(ExperimentSource::try_from_rs("release/not-real/my-slug").is_err());

        Ok(())
    }

    #[test]
    fn test_experiment_source_from_api() -> Result<()> {
        let release = config::api_v6_production_server();
        let stage = config::api_v6_stage_server();
        let slug = "my-slug".to_string();
        assert_eq!(
            ExperimentSource::try_from_api("my-slug")?,
            ExperimentSource::FromApiV6 {
                slug: slug.to_string(),
                endpoint: release.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from_api("release/my-slug")?,
            ExperimentSource::FromApiV6 {
                slug: slug.to_string(),
                endpoint: release.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from_api("stage/my-slug")?,
            ExperimentSource::FromApiV6 {
                slug: slug.to_string(),
                endpoint: stage.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from_api("preview/my-slug")?,
            ExperimentSource::FromApiV6 {
                slug: slug.to_string(),
                endpoint: release.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from_api("release/preview/my-slug")?,
            ExperimentSource::FromApiV6 {
                slug: slug.to_string(),
                endpoint: release
            }
        );
        assert_eq!(
            ExperimentSource::try_from_api("stage/preview/my-slug")?,
            ExperimentSource::FromApiV6 {
                slug,
                endpoint: stage
            }
        );

        Ok(())
    }
}

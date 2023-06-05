// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use anyhow::{Result, bail};
use serde_json::Value;
use std::path::{PathBuf, Path};

use crate::{NimbusApp, cli::{Cli, CliCommand}, value_utils, feature_utils};

use super::ExperimentListSource;

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
}

// Create ExperimentSources from &str and Cli.

impl TryFrom<&str> for ExperimentSource {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        let tokens: Vec<&str> = value.splitn(3, '/').collect();
        let tokens = tokens.as_slice();
        Ok(match tokens {
            [slug] => Self::FromList {
                slug: slug.to_string(),
                list: ExperimentListSource::try_from_pair("", "")?,
            },
            ["preview", slug] => Self::FromList {
                slug: slug.to_string(),
                list: ExperimentListSource::try_from_pair("", "preview")?,
            },
            [server, slug] => Self::FromList {
                slug: slug.to_string(),
                list: ExperimentListSource::try_from_pair(server, "")?,
            },
            [server, "preview", slug] => Self::FromList {
                slug: slug.to_string(),
                list: ExperimentListSource::try_from_pair(server, "preview")?,
            },
            _ => bail!(format!(
                "Can't unpack '{}' into an experiment; try preview/SLUG or stage/SLUG, or stage/preview/SLUG",
                value
            )),
        })
    }
}

impl ExperimentSource {
    pub(crate) fn try_from_file(file: &Path, slug: &str) -> Result<Self> {
        Ok(ExperimentSource::FromList {
            slug: slug.to_string(),
            list: file.try_into()?,
        })
    }
}

impl TryFrom<&Cli> for ExperimentSource {
    type Error = anyhow::Error;

    fn try_from(value: &Cli) -> Result<Self> {
        Ok(match &value.command {
            CliCommand::Enroll { experiment, file, .. } => {
                match file.clone() {
                    Some(file) => Self::try_from_file(&file, experiment)?,
                    _ => Self::try_from(experiment.as_str())?,
                }
            },
            CliCommand::TestFeature { feature_id, files, .. } => {
                Self::FromFeatureFiles {
                    app: value.into(),
                    feature_id: feature_id.clone(),
                    files: files.clone(),
                }
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
    fn test_experiment_source_from_str() -> Result<()> {
        let release = ExperimentListSource::try_from("")?;
        let stage = ExperimentListSource::try_from("stage")?;
        let release_preview = ExperimentListSource::try_from("preview")?;
        let stage_preview = ExperimentListSource::try_from("stage/preview")?;
        let slug = "my-slug".to_string();
        assert_eq!(
            ExperimentSource::try_from("my-slug")?,
            ExperimentSource::FromList {
                list: release.clone(),
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from("release/my-slug")?,
            ExperimentSource::FromList {
                list: release,
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from("stage/my-slug")?,
            ExperimentSource::FromList {
                list: stage,
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from("preview/my-slug")?,
            ExperimentSource::FromList {
                list: release_preview.clone(),
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from("release/preview/my-slug")?,
            ExperimentSource::FromList {
                list: release_preview,
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from("stage/preview/my-slug")?,
            ExperimentSource::FromList {
                list: stage_preview,
                slug
            }
        );

        assert!(ExperimentListSource::try_from("not-real/preview/my-slug").is_err());
        assert!(ExperimentListSource::try_from("release/not-real/my-slug").is_err());

        Ok(())
    }
}

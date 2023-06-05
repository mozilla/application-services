// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::fmt::Display;

use anyhow::Result;
use nimbus_fml::{
    intermediate_representation::FeatureManifest, parser::Parser, util::loaders::FileLoader,
};

use crate::{cli::ManifestArgs, config, NimbusApp};

pub(crate) struct ManifestSource {
    github_repo: String,
    ref_: String,
    manifest_file: String,
    channel: String,
}

impl ManifestSource {
    fn manifest_loader(&self) -> Result<FileLoader> {
        let cwd = std::env::current_dir().expect("Current Working Directory is not set");
        let mut files = FileLoader::new(cwd, config::manifest_cache_dir(), Default::default())?;
        files.add_repo(self.github_repo.as_str(), &self.ref_)?;
        Ok(files)
    }

    pub(crate) fn try_from(params: &NimbusApp, value: &ManifestArgs) -> Result<Self> {
        let github_repo = params.github_repo().to_string();
        let ref_ = params.ref_from_version(&value.version, &value.ref_);
        let manifest_file = value
            .manifest
            .clone()
            .unwrap_or_else(|| format!("@{}/{}", github_repo, params.manifest_location(),));
        let channel = params.channel.clone();
        Ok(Self {
            github_repo,
            ref_,
            channel,
            manifest_file,
        })
    }
}

impl Display for ManifestSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let files = self.manifest_loader().unwrap();
        let path = files.file_path(&self.manifest_file).unwrap();
        f.write_str(&path.to_string())
    }
}

impl TryFrom<&ManifestSource> for FeatureManifest {
    type Error = anyhow::Error;

    fn try_from(value: &ManifestSource) -> Result<Self> {
        let files = value.manifest_loader()?;
        let path = files.file_path(&value.manifest_file)?;
        let parser: Parser = Parser::new(files, path)?;
        let manifest = parser.get_intermediate_representation(&value.channel)?;
        manifest.validate_manifest()?;
        Ok(manifest)
    }
}

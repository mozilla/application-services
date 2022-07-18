/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::parser::AboutBlock;
use anyhow::{bail, Error, Result};
use std::path::Path;
use std::path::PathBuf;

pub(crate) enum CliCmd {
    Generate(GenerateStructCmd),
    DeprecatedGenerate(GenerateStructCmd, AboutBlock),
    GenerateExperimenter(GenerateExperimenterManifestCmd),
    GenerateIR(GenerateIRCmd),
    FetchFile(LoaderConfig, String),
}

#[derive(Clone)]
pub(crate) struct GenerateStructCmd {
    pub(crate) manifest: String,
    pub(crate) output: PathBuf,
    pub(crate) language: TargetLanguage,
    pub(crate) load_from_ir: bool,
    pub(crate) channel: String,
    pub(crate) loader: LoaderConfig,
}

pub(crate) struct GenerateExperimenterManifestCmd {
    pub(crate) manifest: String,
    pub(crate) output: PathBuf,
    pub(crate) language: TargetLanguage,
    pub(crate) load_from_ir: bool,
    pub(crate) channel: String,
    pub(crate) loader: LoaderConfig,
}

pub(crate) struct GenerateIRCmd {
    pub(crate) manifest: String,
    pub(crate) output: PathBuf,
    pub(crate) load_from_ir: bool,
    pub(crate) channel: String,
    pub(crate) loader: LoaderConfig,
}

#[derive(Clone)]
pub(crate) struct LoaderConfig {
    pub(crate) cwd: PathBuf,
    pub(crate) repo_files: Vec<String>,
    pub(crate) cache_dir: PathBuf,
}

impl Default for LoaderConfig {
    fn default() -> Self {
        Self {
            repo_files: Default::default(),
            cache_dir: std::env::temp_dir(),
            cwd: std::env::current_dir().expect("Current Working Directory is not set"),
        }
    }
}

#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub(crate) enum TargetLanguage {
    Kotlin,
    Swift,
    IR,
    ExperimenterYAML,
    ExperimenterJSON,
}

impl TargetLanguage {
    pub(crate) fn extension(&self) -> &str {
        match self {
            TargetLanguage::Kotlin => "kt",
            TargetLanguage::Swift => "swift",
            TargetLanguage::IR => "fml.json",
            TargetLanguage::ExperimenterJSON => "json",
            TargetLanguage::ExperimenterYAML => "yaml",
        }
    }

    pub(crate) fn from_extension(path: &str) -> Result<TargetLanguage> {
        if let Some((_, extension)) = path.rsplit_once('.') {
            extension.try_into()
        } else {
            bail!("Unknown or unsupported target language: \"{}\"", path)
        }
    }
}

impl TryFrom<&str> for TargetLanguage {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self> {
        Ok(match value.to_ascii_lowercase().as_str() {
            "kotlin" | "kt" | "kts" => TargetLanguage::Kotlin,
            "swift" => TargetLanguage::Swift,
            "fml.json" => TargetLanguage::IR,
            "yaml" => TargetLanguage::ExperimenterYAML,
            "json" => TargetLanguage::ExperimenterJSON,
            _ => bail!("Unknown or unsupported target language: \"{}\"", value),
        })
    }
}

impl TryFrom<&std::ffi::OsStr> for TargetLanguage {
    type Error = Error;
    fn try_from(value: &std::ffi::OsStr) -> Result<Self> {
        if let Some(s) = value.to_str() {
            TryFrom::try_from(s)
        } else {
            bail!("Unreadable target language")
        }
    }
}

impl TryFrom<&Path> for TargetLanguage {
    type Error = Error;
    fn try_from(value: &Path) -> Result<Self> {
        TryFrom::try_from(
            value
                .extension()
                .ok_or_else(|| anyhow::anyhow!("No extension available to determine language"))?,
        )
    }
}

impl TryFrom<String> for TargetLanguage {
    type Error = Error;
    fn try_from(value: String) -> Result<Self> {
        TryFrom::try_from(value.as_str())
    }
}

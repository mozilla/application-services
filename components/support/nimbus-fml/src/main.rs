/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[macro_use]
extern crate clap;

mod backends;
mod error;
#[cfg(test)]
#[allow(dead_code)]
mod fixtures;
mod intermediate_representation;
mod parser;
mod util;
mod workflows;

use anyhow::{bail, Result};
use clap::{App, ArgMatches};
use parser::{AboutBlock, KotlinAboutBlock, SwiftAboutBlock};

use std::path::{Path, PathBuf};

const RELEASE_CHANNEL: &str = "release";
const SUPPORT_URL_LOADING: bool = false;

fn main() -> Result<()> {
    let yaml = load_yaml!("cli.yaml");
    let matches = App::from_yaml(yaml).get_matches();
    let cwd = std::env::current_dir()?;

    match matches.subcommand() {
        ("android", Some(cmd)) => match cmd.subcommand() {
            ("features", Some(cmd)) => {
                let (class, package, rpackage) = (
                    cmd.value_of("class_name"),
                    cmd.value_of("package"),
                    cmd.value_of("r_package"),
                );
                let config = match (class, package, rpackage) {
                    (Some(class_name), Some(class_package), Some(package_id)) => {
                        Some(KotlinAboutBlock {
                            class: format!("{}.{}", class_package, class_name),
                            package: package_id.to_string(),
                        })
                    }
                    (None, None, None) => None,
                    _ => None,
                };
                workflows::generate_struct_cli_overrides(
                    AboutBlock {
                        kotlin_about: config,
                        ..Default::default()
                    },
                    &GenerateStructCmd {
                        language: TargetLanguage::Kotlin,
                        manifest: file_path("INPUT", &matches, &cwd)?,
                        output: file_path("output", &matches, &cwd)?,
                        load_from_ir: matches.is_present("ir"),
                        channel: matches
                            .value_of("channel")
                            .map(str::to_string)
                            .unwrap_or_else(|| RELEASE_CHANNEL.into()),
                    },
                )?
            }
            _ => unimplemented!(),
        },
        ("ios", Some(cmd)) => match cmd.subcommand() {
            ("features", Some(cmd)) => {
                let (class, module) = (cmd.value_of("class_name"), cmd.value_of("module_name"));
                let config = match (class, module) {
                    (Some(class_name), Some(module_name)) => Some(SwiftAboutBlock {
                        class: class_name.to_string(),
                        module: module_name.to_string(),
                    }),
                    (Some(class_name), _) => Some(SwiftAboutBlock {
                        class: class_name.to_string(),
                        module: "Application".to_string(),
                    }),
                    (None, None) => None,
                    _ => None,
                };
                workflows::generate_struct_cli_overrides(
                    AboutBlock {
                        swift_about: config,
                        ..Default::default()
                    },
                    &GenerateStructCmd {
                        language: TargetLanguage::Swift,
                        manifest: file_path("INPUT", &matches, &cwd)?,
                        output: file_path("output", &matches, &cwd)?,
                        load_from_ir: matches.is_present("ir"),
                        channel: matches
                            .value_of("channel")
                            .map(str::to_string)
                            .unwrap_or_else(|| RELEASE_CHANNEL.into()),
                    },
                )?
            }
            _ => unimplemented!(),
        },
        ("experimenter", _) => {
            workflows::generate_experimenter_manifest(GenerateExperimenterManifestCmd {
                manifest: file_path("INPUT", &matches, &cwd)?,
                output: file_path("output", &matches, &cwd)?,
                load_from_ir: matches.is_present("ir"),
                channel: matches
                    .value_of("channel")
                    .map(str::to_string)
                    .unwrap_or_else(|| RELEASE_CHANNEL.into()),
            })?
        }
        ("intermediate-repr", _) => workflows::generate_ir(GenerateIRCmd {
            manifest: file_path("INPUT", &matches, &cwd)?,
            output: file_path("output", &matches, &cwd)?,
            load_from_ir: matches.is_present("ir"),
            channel: matches
                .value_of("channel")
                .map(str::to_string)
                .unwrap_or_else(|| RELEASE_CHANNEL.into()),
        })?,
        (word, _) => unimplemented!("Command {} not implemented", word),
    };

    Ok(())
}

fn file_path(name: &str, args: &ArgMatches, cwd: &Path) -> Result<PathBuf> {
    let mut abs = cwd.to_path_buf();
    match args.value_of(name) {
        Some(suffix) => {
            abs.push(suffix);
            Ok(abs)
        }
        _ => bail!("A file path is needed for {}", name),
    }
}

pub struct GenerateStructCmd {
    manifest: PathBuf,
    output: PathBuf,
    language: TargetLanguage,
    load_from_ir: bool,
    channel: String,
}

pub struct GenerateExperimenterManifestCmd {
    manifest: PathBuf,
    output: PathBuf,
    load_from_ir: bool,
    channel: String,
}

pub struct GenerateIRCmd {
    manifest: PathBuf,
    output: PathBuf,
    load_from_ir: bool,
    channel: String,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub enum TargetLanguage {
    Kotlin,
    Swift,
    IR,
    ExperimenterYAML,
    ExperimenterJSON,
}

impl TargetLanguage {
    #[allow(dead_code)]
    pub(crate) fn extension(&self) -> &str {
        match self {
            TargetLanguage::Kotlin => "kt",
            TargetLanguage::Swift => "swift",
            TargetLanguage::IR => "fml.json",
            TargetLanguage::ExperimenterJSON => "json",
            TargetLanguage::ExperimenterYAML => "yaml",
        }
    }
}

impl TryFrom<&str> for TargetLanguage {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self> {
        Ok(match value.to_ascii_lowercase().as_str() {
            "kotlin" | "kt" | "kts" => TargetLanguage::Kotlin,
            "swift" => TargetLanguage::Swift,
            "yaml" => TargetLanguage::ExperimenterYAML,
            "json" => TargetLanguage::ExperimenterJSON,
            _ => bail!("Unknown or unsupported target language: \"{}\"", value),
        })
    }
}

impl TryFrom<&std::ffi::OsStr> for TargetLanguage {
    type Error = anyhow::Error;
    fn try_from(value: &std::ffi::OsStr) -> Result<Self> {
        match value.to_str() {
            None => bail!("Unreadable target language"),
            Some(s) => s.try_into(),
        }
    }
}

impl TryFrom<String> for TargetLanguage {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self> {
        TryFrom::try_from(value.as_str())
    }
}

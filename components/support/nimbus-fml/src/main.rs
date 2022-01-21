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
use serde::Deserialize;

use std::{
    convert::{TryFrom, TryInto},
    path::{Path, PathBuf},
};

const RELEASE_CHANNEL: &str = "release";

fn main() -> Result<()> {
    let yaml = load_yaml!("cli.yaml");
    let matches = App::from_yaml(yaml).get_matches();
    let cwd = std::env::current_dir()?;

    let config = if matches.is_present("config") {
        util::slurp_config(&file_path("config", &matches, &cwd)?)?
    } else {
        Default::default()
    };

    match matches.subcommand() {
        ("android", Some(cmd)) => match cmd.subcommand() {
            ("features", Some(cmd)) => workflows::generate_struct(
                Config {
                    nimbus_object_name: cmd
                        .value_of("class_name")
                        .map(str::to_string)
                        .or(config.nimbus_object_name),
                    nimbus_package: cmd
                        .value_of("package")
                        .map(str::to_string)
                        .or(config.nimbus_package),
                    resource_package: cmd
                        .value_of("r_package")
                        .map(str::to_string)
                        .or(config.resource_package),
                },
                GenerateStructCmd {
                    language: TargetLanguage::Kotlin,
                    manifest: file_path("INPUT", &matches, &cwd)?,
                    output: file_path("output", &matches, &cwd)?,
                    load_from_ir: matches.is_present("ir"),
                    channel: matches
                        .value_of("channel")
                        .map(str::to_string)
                        .unwrap_or_else(|| RELEASE_CHANNEL.into()),
                },
            )?,
            _ => unimplemented!(),
        },
        ("ios", Some(cmd)) => match cmd.subcommand() {
            ("features", Some(cmd)) => workflows::generate_struct(
                Config {
                    nimbus_object_name: cmd
                        .value_of("class_name")
                        .map(str::to_string)
                        .or(config.nimbus_object_name),
                    ..Default::default()
                },
                GenerateStructCmd {
                    language: TargetLanguage::Swift,
                    manifest: file_path("INPUT", &matches, &cwd)?,
                    output: file_path("output", &matches, &cwd)?,
                    load_from_ir: matches.is_present("ir"),
                    channel: matches
                        .value_of("channel")
                        .map(str::to_string)
                        .unwrap_or_else(|| RELEASE_CHANNEL.into()),
                },
            )?,
            _ => unimplemented!(),
        },
        ("experimenter", _) => workflows::generate_experimenter_manifest(
            config,
            GenerateExperimenterManifestCmd {
                manifest: file_path("INPUT", &matches, &cwd)?,
                output: file_path("output", &matches, &cwd)?,
                load_from_ir: matches.is_present("ir"),
                channel: matches
                    .value_of("channel")
                    .map(str::to_string)
                    .unwrap_or_else(|| RELEASE_CHANNEL.into()),
            },
        )?,
        ("intermediate-repr", _) => workflows::generate_ir(
            config,
            GenerateIRCmd {
                manifest: file_path("INPUT", &matches, &cwd)?,
                output: file_path("output", &matches, &cwd)?,
                load_from_ir: matches.is_present("ir"),
                channel: matches
                    .value_of("channel")
                    .map(str::to_string)
                    .unwrap_or_else(|| RELEASE_CHANNEL.into()),
            },
        )?,
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

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub nimbus_package: Option<String>,
    pub nimbus_object_name: Option<String>,
    pub resource_package: Option<String>,
}


impl Config {
    fn package_name(&self) -> Option<String> {
        self.package_name.clone()
    }

    fn nimbus_object_name(&self) -> String {
        self.nimbus_object_name
            .clone()
            .unwrap_or_else(|| "MyNimbus".into())
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
}

impl TargetLanguage {
    #[allow(dead_code)]
    pub(crate) fn extension(&self) -> &str {
        match self {
            TargetLanguage::Kotlin => "kt",
            TargetLanguage::Swift => "swift",
            TargetLanguage::IR => "fml.json",
        }
    }
}

impl TryFrom<&str> for TargetLanguage {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self> {
        Ok(match value.to_ascii_lowercase().as_str() {
            "kotlin" | "kt" | "kts" => TargetLanguage::Kotlin,
            "swift" => TargetLanguage::Swift,
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

/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[macro_use]
extern crate clap;

mod error;
#[cfg(test)]
#[allow(dead_code)]
mod fixtures;
mod intermediate_representation;
mod parser;
mod workflows;

use crate::error::{FMLError, Result};
use clap::{App, ArgMatches};
use serde::Deserialize;

use std::{
    convert::{TryFrom, TryInto},
    path::{Path, PathBuf},
};

fn main() -> Result<()> {
    let yaml = load_yaml!("cli.yaml");
    let matches = App::from_yaml(yaml).get_matches();
    let cwd = std::env::current_dir()?;

    let config = if matches.is_present("config") {
        Some(file_path("config", &matches, &cwd)?)
    } else {
        None
    };

    match matches.subcommand() {
        ("struct", Some(cmd)) => workflows::generate_struct(
            config,
            GenerateStructCmd {
                manifest: file_path("INPUT", cmd, &cwd)?,
                output: file_path("output", cmd, &cwd)?,
                language: cmd
                    .value_of("language")
                    .expect("Language is required")
                    .try_into()?,
                load_from_ir: !cmd.is_present("ir"),
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
        _ => Err(FMLError::InvalidPath(name.into())),
    }
}

#[derive(Debug, Deserialize)]
pub struct Config {}

pub struct GenerateStructCmd {
    manifest: PathBuf,
    output: PathBuf,
    language: TargetLanguage,
    load_from_ir: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub enum TargetLanguage {
    Kotlin,
    Swift,
    IR,
}

impl TryFrom<&str> for TargetLanguage {
    type Error = error::FMLError;
    fn try_from(value: &str) -> Result<Self> {
        Ok(match value.to_ascii_lowercase().as_str() {
            "kotlin" | "kt" | "kts" => TargetLanguage::Kotlin,
            "swift" => TargetLanguage::Swift,
            "ir" => TargetLanguage::IR,
            _ => {
                return Err(FMLError::CLIError(format!(
                    "Unimplemented language: {}",
                    value
                )))
            }
        })
    }
}

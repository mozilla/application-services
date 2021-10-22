/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[macro_use]
extern crate clap;
use clap::{App, ArgMatches};
use nimbus_fml::error::{FMLError, Result};
use nimbus_fml::intermediate_representation::FeatureManifest;
use nimbus_fml::parser::Parser;
use std::path::Path;
use std::{fs::File, path::PathBuf};

fn main() -> Result<()> {
    let yaml = load_yaml!("cli.yaml");
    let matches = App::from_yaml(yaml).get_matches();
    let cwd = std::env::current_dir()?;
    if let Some(cmd) = matches.subcommand_matches("struct") {
        let manifest_file_path = file_path("INPUT", cmd, &cwd)?;

        let ir = if !cmd.is_present("ir") {
            let file = File::open(manifest_file_path)?;
            let _parser: Parser = Parser::new(file);
            unimplemented!("No parser is available")
        } else {
            let string = slurp_file(&manifest_file_path)?;
            serde_json::from_str::<FeatureManifest>(&string)?
        };

        let output_path = file_path("output", cmd, &cwd)?;
        match cmd.value_of("language") {
            Some("ir") => {
                let contents = serde_json::to_string_pretty(&ir)?;
                std::fs::write(output_path, contents)?;
            }
            _ => unimplemented!("Language not implemented yet"),
        };
    }
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

fn slurp_file(file_name: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(file_name)?)
}

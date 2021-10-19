/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use clap::{App, Arg};
use nimbus_fml::parser::Parser;
use std::fs::File;
fn main() -> anyhow::Result<()> {
    let matches = App::new("Nimbus Feature Manifest")
        .version("0.1.0")
        .author("Nimbus SDK Engineering")
        .about("A tool to generate code using an experiment feature manifest")
        .arg(
            Arg::with_name("manifest")
                .short("m")
                .long("manifest")
                .value_name("FILE")
                .help("Sets the manifest file to use")
                .required(true)
                .takes_value(true),
        )
        .get_matches();
    let manifest_file_path = matches
        .value_of("manifest")
        .expect("Manifest path is required, but not found");
    let file = File::open(manifest_file_path)?;
    let _parser = Parser::new(file);
    Ok(())
}

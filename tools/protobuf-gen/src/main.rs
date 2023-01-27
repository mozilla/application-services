/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use clap::{App, Arg};
use serde_derive::Deserialize;
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Deserialize, Debug)]
struct ProtobufOpts {
    dir: String,
    out_dir: Option<String>,
}

fn main() {
    let matches = App::new("Protobuf files generator")
        .about("Generates Rust structs from protobuf files in the repository.")
        .arg(
            Arg::with_name("PROTOBUF_CONFIG")
                .help("Absolute path to the protobuf configuration file.")
                .required(true),
        )
        .get_matches();
    let config_path = matches.value_of("PROTOBUF_CONFIG").unwrap();
    let config_path = PathBuf::from(config_path);
    let files_config = fs::read_to_string(&config_path).expect("unable to read protobuf_files");
    let files: HashMap<String, ProtobufOpts> = toml::from_str(&files_config).unwrap();
    let config_dir = config_path.parent().unwrap();

    for (proto_file, opts) in files {
        // Can't re-use Config because the out_dir is always different.
        let mut config = prost_build::Config::new();
        let out_dir = opts.out_dir.clone().unwrap_or_else(|| opts.dir.clone());
        let out_dir_absolute = config_dir.join(out_dir).canonicalize().unwrap();
        let out_dir_absolute = out_dir_absolute.to_str().unwrap();
        let proto_path_absolute = config_dir
            .join(&opts.dir)
            .join(proto_file)
            .canonicalize()
            .unwrap();
        let proto_path_absolute = proto_path_absolute.to_str().unwrap();
        let include_dir_absolute = config_dir.join(&opts.dir).canonicalize().unwrap();
        let include_dir_absolute = include_dir_absolute.to_str().unwrap();
        config.out_dir(out_dir_absolute);
        config
            .compile_protos(&[proto_path_absolute], &[&include_dir_absolute])
            .unwrap();
    }
}

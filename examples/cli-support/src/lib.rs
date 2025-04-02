/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![allow(unknown_lints)]
#![warn(rust_2018_idioms)]

use std::path::{Path, PathBuf};

pub mod fxa_creds;
pub mod prompt;

pub use env_logger;

pub fn init_logging_with(s: &str) {
    let noisy = "tokio_threadpool=warn,tokio_reactor=warn,tokio_core=warn,tokio=warn,hyper=warn,want=warn,mio=warn,reqwest=warn";
    let spec = format!("{},{}", s, noisy);
    env_logger::init_from_env(env_logger::Env::default().filter_or("RUST_LOG", spec));
}

pub fn init_trace_logging() {
    init_logging_with("trace")
}

pub fn init_logging() {
    init_logging_with(if cfg!(debug_assertions) {
        "debug"
    } else {
        "info"
    })
}

pub fn cli_data_dir() -> String {
    data_path(None).to_string_lossy().to_string()
}

pub fn cli_data_subdir(relative_path: &str) -> String {
    data_path(Some(relative_path)).to_string_lossy().to_string()
}

pub fn cli_data_path(filename: &str) -> String {
    data_path(None).join(filename).to_string_lossy().to_string()
}

fn data_path(relative_path: Option<&str>) -> PathBuf {
    let dir = workspace_root_dir().join(".cli-data");
    match relative_path {
        None => dir,
        Some(relative_path) => dir.join(relative_path),
    }
}

pub fn workspace_root_dir() -> PathBuf {
    let cargo_output = std::process::Command::new(env!("CARGO"))
        .arg("locate-project")
        .arg("--workspace")
        .arg("--message-format=plain")
        .output()
        .unwrap()
        .stdout;
    let cargo_toml_path = Path::new(std::str::from_utf8(&cargo_output).unwrap().trim());
    cargo_toml_path.parent().unwrap().to_path_buf()
}

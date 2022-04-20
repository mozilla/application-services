/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{env, process::Command};

pub fn main() {
    #[cfg(feature = "uniffi-bindings")]
    uniffi_build::generate_scaffolding("./src/nimbus.udl").unwrap();

    // Run Glean Parser via tools/bootstrap_glean_rust.py python script
    let out_dir = env::var("OUT_DIR").unwrap();
    Command::new("python")
        .args(&[
            "../../tools/bootstrap_glean_rust.py",
            "online",
            "glean_parser",
            "5.1.1",
            "translate",
            "-f",
            "rust",
            "-o",
            out_dir.as_str(),
            "./metrics.yaml",
        ])
        .status()
        .expect("Error generating Glean Rust bindings");
}

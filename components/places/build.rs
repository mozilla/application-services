/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use glean_build::Builder;

fn main() {
    uniffi::generate_scaffolding("./src/places.udl").unwrap();

    Builder::default()
        .file("metrics.yaml")
        .format("rust_sym")
        .generate()
        .expect("Error generating Glean Rust bindings");
}

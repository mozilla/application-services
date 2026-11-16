/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

/// Metrics files to process
const METRICS_FILES: &[&str] = &[
    "components/logins/metrics.yaml",
    "components/places/metrics.yaml",
    "components/support/error/metrics.yaml",
    "components/support/error/pings.yaml",
];

/// Generate Glean metrics code
fn main() {
    let format = if cfg!(feature = "fog") {
        "rust"
    } else {
        "rust_sym"
    };

    glean_build::Builder::default()
        .files(METRICS_FILES.iter().map(|s| format!("../../../{s}")))
        .format(format)
        .generate()
        .expect("Error generating Glean Rust bindings");
}

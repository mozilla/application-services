/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

fn env(name: &str) -> Option<OsString> {
    println!("cargo:rerun-if-env-changed={}", name);
    env::var_os(name)
}

fn main() {
    // See https://kazlauskas.me/entries/writing-proper-buildrs-scripts.html.
    let target_os = env::var("CARGO_CFG_TARGET_OS");
    // Only iOS dynamically links with NSS. All the other platforms dlopen.
    if let Ok("ios") = target_os.as_ref().map(|x| &**x) {
        let (lib_dir, include_dir) = get_nss();
        println!(
            "cargo:rustc-link-search=native={}",
            lib_dir.to_string_lossy()
        );
        println!("cargo:include={}", include_dir.to_string_lossy());
    }
}

pub fn get_nss() -> (PathBuf, PathBuf) {
    let nss_dir = env("NSS_DIR").expect("To build for iOS, NSS_DIR must be set!");
    let nss_dir = Path::new(&nss_dir);
    let lib_dir = nss_dir.join("lib");
    let include_dir = nss_dir.join("include");
    (lib_dir, include_dir)
}

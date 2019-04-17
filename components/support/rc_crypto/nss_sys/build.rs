/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use bindgen::Builder;
use serde_derive::Deserialize;
use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use toml;

const BINDINGS_CONFIG: &'static str = "bindings.toml";

// This is the format of a single section of the configuration file.
#[derive(Deserialize)]
struct Bindings {
    // The .h header files to generate from.
    headers: Vec<String>,
    // types that are explicitly included
    types: Option<Vec<String>>,
    // (un-used) functions that are explicitly included
    // functions: Option<Vec<String>>,
    // variables (and `#define`s) that are explicitly included
    variables: Option<Vec<String>>,
    // types that should be explicitly marked as opaque
    opaque: Option<Vec<String>>,
    // enumerations that are turned into a module (without this, the enum is
    // mapped using the default, which means that the individual values are
    // formed with an underscore as <enum_type>_<enum_value_name>).
    enums: Option<Vec<String>>,

    // Any item that is specifically excluded; if none of the types, functions,
    // or variables fields are specified, everything defined will be mapped,
    // so this can be used to limit that.
    exclude: Option<Vec<String>>,
}

fn env(name: &str) -> Option<OsString> {
    println!("cargo:rerun-if-env-changed={}", name);
    env::var_os(name)
}

fn main() {
    let (lib_dir, include_dir) = get_nss();
    // See https://kazlauskas.me/entries/writing-proper-buildrs-scripts.html.
    let target_os = env::var("CARGO_CFG_TARGET_OS");
    // Only iOS dynamically links with NSS. All the other platforms dlopen.
    if let Ok("ios") = target_os.as_ref().map(|x| &**x) {
        println!(
            "cargo:rustc-link-search=native={}",
            lib_dir.to_string_lossy()
        );
        println!("cargo:include={}", include_dir.to_string_lossy());
    }
    let config_file = PathBuf::from(BINDINGS_CONFIG);
    println!("cargo:rerun-if-changed={}", config_file.to_str().unwrap());
    let config = fs::read_to_string(config_file).expect("unable to read binding configuration");
    let bindings: Bindings = toml::from_str(&config).unwrap();
    build_bindings(&bindings, &include_dir.join("nss"));
}

pub fn get_nss() -> (PathBuf, PathBuf) {
    let nss_dir = env("NSS_DIR").expect("To build nss_sys, NSS_DIR must be set!");
    let nss_dir = Path::new(&nss_dir);
    let lib_dir = nss_dir.join("lib");
    let include_dir = nss_dir.join("include");
    (lib_dir, include_dir)
}

fn build_bindings(bindings: &Bindings, include_dir: &PathBuf) {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("nss_bindings.rs");
    let mut builder = Builder::default().generate_comments(false);

    for h in bindings.headers.iter().cloned() {
        builder = builder.header(include_dir.join(h).to_str().unwrap());
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS");
    // Workaround cross-compilation bugs for iOS.
    if let Ok("ios") = target_os.as_ref().map(|x| &**x) {
        builder = builder.detect_include_paths(false);
        let target_arch = env::var("CARGO_CFG_TARGET_ARCH");
        let sdk_root;
        match target_arch.as_ref().map(|x| &**x).unwrap() {
            "aarch64" => {
                sdk_root = get_ios_sdk_root("iphoneos");
                builder = builder.clang_arg("--target=arm64-apple-ios") // See https://github.com/rust-lang/rust-bindgen/issues/1211
            }
            "x86_64" => {
                sdk_root = get_ios_sdk_root("iphonesimulator");
            }
            _ => panic!("Unknown iOS architecture."),
        }
        builder = builder.clang_arg(format!("-isysroot{}", &sdk_root));
    }

    // Apply the configuration.
    let empty: Vec<String> = vec![];
    for v in bindings.types.as_ref().unwrap_or_else(|| &empty).iter() {
        builder = builder.whitelist_type(v);
    }
    // for v in bindings.functions.as_ref().unwrap_or_else(|| &empty).iter() {
    //     builder = builder.whitelist_function(v);
    // }
    for v in bindings.variables.as_ref().unwrap_or_else(|| &empty).iter() {
        builder = builder.whitelist_var(v);
    }
    for v in bindings.exclude.as_ref().unwrap_or_else(|| &empty).iter() {
        builder = builder.blacklist_item(v);
    }
    for v in bindings.opaque.as_ref().unwrap_or_else(|| &empty).iter() {
        builder = builder.opaque_type(v);
    }
    for v in bindings.enums.as_ref().unwrap_or_else(|| &empty).iter() {
        builder = builder.constified_enum_module(v);
    }

    let bindings = builder.generate().expect("unable to generate bindings");
    bindings
        .write_to_file(out)
        .expect("couldn't write bindings");
}

pub fn get_ios_sdk_root(sdk_name: &str) -> String {
    let output = Command::new("xcrun")
        .arg("--show-sdk-path")
        .arg("-sdk")
        .arg(sdk_name)
        .output()
        .unwrap();
    if output.status.success() {
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    } else {
        panic!("Could not get iOS SDK root!")
    }
}

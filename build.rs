/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::env;
use std::fs::canonicalize;
use std::path::Path;
use std::process::Command;

fn main() {
    // uncomment to print all env vars
    for (k, v) in std::env::vars() {
        println!("{} -> {}", k, v);
    }

    let out_dir = env::var("OUT_DIR").unwrap();

    // /Users/nalexander/Mozilla/application-services/target/debug/build/logins_ffi-d0290628a891f004/out
    let target = Path::new(&out_dir).ancestors().find(|p| p.ends_with("target")).unwrap();

    // path.ends_with("passwd"));

    // run the build scripts from the lib dir.
    let libs_dir =
        canonicalize(target.join("..").join("build").join("libs")).unwrap();

    // canonicalize(Path::new(".").join("build").join("libs")).unwrap();
    // canonicalize(Path::new(".").join("..").join("..").join("..").join("build").join("libs")).unwrap();

    let target_os = env::var("CARGO_CFG_TARGET_OS");
    let build_info = match target_os.as_ref().map(|x| &**x) {
        Ok("linux") => Some(("desktop", "linux-x86-64")),
        Ok("windows") => Some(("desktop", "win32-x86-64")),
        Ok("macos") => Some(("desktop", "darwin")),
        //        Ok("android") => Some("android"), - TODO - need to do better at x-compile support.
        _ => None,
    };
    match build_info {
        None => println!(
            "Unknown target OS '{:?}' - not executing external build script",
            target_os
        ),
        Some((build_platform, target_dir)) => {
            // let status = Command::new("sh")
            //     .args(&["-c", &format!("./build-all.sh {}", build_platform)])
            //     .current_dir(libs_dir.as_os_str())
            //     .status()
            //     .unwrap();
            // if !status.success() {
            //     panic!("external build script failed: {:?}", status);
            // }
            // println!("external build script succeeded");

            println!("cargo:rustc-env=OPENSSL_STATIC=1");

            let openssl = libs_dir
                .join(build_platform)
                .join(target_dir)
                .join("openssl");
            println!("cargo:rustc-env=OPENSSL_DIR={}", openssl.to_str().unwrap());

            let sqlcipher = libs_dir
                .join(build_platform)
                .join(target_dir)
                .join("sqlcipher");
            println!("cargo:rustc-env=SQLCIPHER_LIB_DIR={}", sqlcipher.join("lib").to_str().unwrap());
            println!("cargo:rustc-env=SQLCIPHER_INCLUDE_DIR={}", sqlcipher.join("include").to_str().unwrap());

            // // point at the libraries
            // let openssl = libs_dir
            //     .join(build_platform)
            //     .join(target_dir)
            //     .join("openssl")
            //     .join("lib");
            // println!(
            //     "cargo:rustc-link-search=native={}",
            //     openssl.to_str().unwrap()
            // );

            // let sqlcipher = libs_dir
            //     .join(build_platform)
            //     .join(target_dir)
            //     .join("sqlcipher")
            //     .join("lib");
            // println!(
            //     "cargo:rustc-link-search=native={}",
            //     sqlcipher.to_str().unwrap()
            // );
        }
    };
}

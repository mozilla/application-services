/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    fs::{read_to_string, File},
    io::Write,
    process::Command,
};

use anyhow::Result;
use camino::Utf8Path;

use crate::{
    cargo_metadata::CargoMetadataInfo,
    toml::{add_cargo_toml_dependency, update_uniffi_toml},
};

pub fn generate_ios(crate_name: String) -> Result<()> {
    generate(crate_name, IosMegazord::Ios)
}

pub fn generate_ios_focus(crate_name: String) -> Result<()> {
    generate(crate_name, IosMegazord::Focus)
}

enum IosMegazord {
    Ios,
    Focus,
}

impl IosMegazord {
    fn root_dir<'a>(&self, metadata_info: &'a CargoMetadataInfo) -> &'a Utf8Path {
        match self {
            Self::Ios => &metadata_info.ios_megazord_root,
            Self::Focus => &metadata_info.ios_focus_megazord_root,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Ios => "Ios",
            Self::Focus => "Ios Focus",
        }
    }

    fn crate_name(&self) -> &'static str {
        match self {
            Self::Ios => "megazord_ios",
            Self::Focus => "megazord_focus",
        }
    }
}

fn generate(crate_name: String, megazord: IosMegazord) -> Result<()> {
    let metadata = CargoMetadataInfo::new(&crate_name)?;
    add_cargo_toml_dependency(
        megazord.root_dir(&metadata),
        &metadata.crate_root,
        &crate_name,
    )?;
    update_uniffi_toml(
        &metadata.crate_root,
        "swift",
        [
            ("ffi_module_name", "MozillaRustComponents".into()),
            ("ffi_module_filename", format!("{crate_name}FFI").into()),
        ],
    )?;
    update_megazord_lib_rs(
        megazord.root_dir(&metadata),
        megazord.crate_name(),
        &crate_name,
    )?;
    println!();
    println!("{} bindings successfully started!", megazord.name());
    println!();
    println!(
        "The next step is to update the iOS Xcode project.  See the application-services docs:"
    );
    println!("https://mozilla.github.io/application-services/book/howtos/adding-a-new-component.html#adding-your-component-to-the-swift-package-manager-megazord");
    println!();
    println!("Optional steps:");
    println!(
        " - Add hand-written code in {}",
        metadata
            .crate_root
            .join("ios")
            .strip_prefix(&metadata.workspace_root)
            .unwrap()
    );
    Ok(())
}

/// Add `pub use <crate>` to lib.rs for the megazord.
///
/// This is needed for iOS, but not for Android.  Maybe because iOS uses a static lib.
fn update_megazord_lib_rs(
    crate_root: &Utf8Path,
    megazord_crate_name: &str,
    crate_name: &str,
) -> Result<()> {
    let path = crate_root.join("src").join("lib.rs");
    let contents = read_to_string(&path)?;
    let mut lines: Vec<_> = contents.split('\n').collect();
    let new_use_statement = format!("pub use {crate_name};");

    let mut last_pub_use = None;
    for (i, line) in lines.iter().enumerate() {
        if line.trim() == new_use_statement {
            // The use statement is already present, don't change anything
            return Ok(());
        } else if line.trim().starts_with("pub use") {
            last_pub_use = Some(i);
        }
    }
    let insert_pos = match last_pub_use {
        Some(i) => i + 1,
        None => lines.len(),
    };
    lines.insert(insert_pos, &new_use_statement);
    let mut file = File::create(&path)?;
    write!(file, "{}", lines.join("\n"))?;
    println!("{path} generated");

    // Run cargo fmt to ensure the imports are sorted in the correct order.
    Command::new("cargo")
        .args(["fmt", "-p", megazord_crate_name])
        .spawn()?
        .wait()?;

    Ok(())
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{
    fs::{create_dir_all, read_to_string, File},
    io::Write,
    process::Command,
};

use anyhow::{anyhow, bail, Result};
use camino::Utf8Path;
use rinja::Template;

use crate::{
    cargo_metadata::CargoMetadataInfo,
    toml::{add_cargo_toml_dependency, update_uniffi_toml},
};

pub fn generate_android(crate_name: String, description: String) -> Result<()> {
    let metadata = CargoMetadataInfo::new(&crate_name)?;
    let android_root = metadata.crate_root.join("android");

    println!();
    write_file(
        BuildGradle {
            crate_name: crate_name.clone(),
        }
        .render()?,
        &android_root.join("build.gradle"),
    )?;
    write_file(
        ANDROID_MANIFEST,
        &android_root
            .join("src")
            .join("main")
            .join("AndroidManifest.xml"),
    )?;
    write_file(PROGUARD_RULES, &android_root.join("proguard-rules.pro"))?;
    update_uniffi_toml(
        &metadata.crate_root,
        "kotlin",
        [(
            "package_name",
            format!("mozilla.appservices.{crate_name}").into(),
        )],
    )?;
    add_cargo_toml_dependency(
        &metadata.android_megazord_root,
        &metadata.crate_root,
        &crate_name,
    )?;
    update_buildconfig(
        &metadata.workspace_root,
        &crate_name,
        &android_root,
        &description,
    )?;
    update_megazord_lib_rs(
        &metadata.android_megazord_root.join("src/lib.rs"),
        &crate_name,
    )?;

    println!();
    println!("Android bindings successfully started!");
    println!();
    println!("Run `./gradlew <your_crate_name>:assembleDebug` from the app-services root directory to test that this is working");
    println!();
    println!("Does crate use types from another crate in it's public API?  If so, you'll need to tweak the `android/build.gradle` file:");
    println!("https://mozilla.github.io/application-services/book/howtos/adding-a-new-component.html#dependent-crates");
    println!();
    println!("Optional steps:");
    println!(
        " - Add hand-written Android code in {}",
        metadata
            .crate_root
            .join("android")
            .join("src")
            .join("main")
            .join("java")
            .join("mozilla")
            .join("appservices")
            .join(&crate_name)
            .strip_prefix(&metadata.workspace_root)
            .unwrap()
    );
    println!(
        " - Add tests in {}",
        metadata
            .crate_root
            .join("android")
            .join("src")
            .join("test")
            .join("java")
            .join("mozilla")
            .join("appservices")
            .join(&crate_name)
            .strip_prefix(&metadata.workspace_root)
            .unwrap()
    );

    Ok(())
}

fn write_file(contents: impl AsRef<str>, path: &Utf8Path) -> Result<()> {
    let contents = contents.as_ref();
    create_dir_all(path.parent().unwrap())?;

    let mut file = File::create(path)?;
    writeln!(file, "{contents}")?;
    println!("{path} generated");

    Ok(())
}

fn update_megazord_lib_rs(lib_path: &Utf8Path, crate_name: &str) -> Result<()> {
    let content = read_to_string(lib_path)?;
    let mut lines: Vec<String> = content.split("\n").map(str::to_string).collect();
    let first_use_line = lines
        .iter()
        .position(|line| line.starts_with("pub use"))
        .ok_or_else(|| anyhow!("Couldn't find a `pub use` line in {lib_path}"))?;
    lines.insert(first_use_line, format!("pub use {crate_name};"));
    write_file(lines.join("\n"), lib_path)?;
    Command::new("cargo")
        .args(["fmt", "-pmegazord"])
        .spawn()?
        .wait()?;

    Ok(())
}

// Update .buildconfig-android.yml
//
// We don't have anything like toml-edit that can edit YAML files while maintaining the formatting.
// Instead, if we need to update the file, append a manually constructed YAML fragment.
fn update_buildconfig(
    workspace_root: &Utf8Path,
    crate_name: &str,
    android_root: &Utf8Path,
    description: &str,
) -> Result<()> {
    let path = workspace_root.join(".buildconfig-android.yml");
    if !buildconfig_needs_update(&path, crate_name)? {
        println!("{path} skipped ([projects.{crate_name}] key already exists)");
        return Ok(());
    }

    let fragment = BuildConfigFragementTemplate {
        crate_name: crate_name.to_owned(),
        android_root: android_root
            .strip_prefix(workspace_root)
            .unwrap()
            .to_string(),
        description: description.to_owned(),
    }
    .render()?;

    let mut file = File::options().append(true).open(&path)?;
    writeln!(file, "{fragment}")?;
    println!("{path} updated");

    Ok(())
}

fn buildconfig_needs_update(path: &Utf8Path, crate_name: &str) -> Result<bool> {
    let config: serde_yaml::Value = serde_yaml::from_str(&read_to_string(path)?)?;
    let projects = config
        .as_mapping()
        .and_then(|m| m.get(&"projects".into()))
        .and_then(|v| v.as_mapping());
    match projects {
        None => bail!("buildconfig.yaml does not have projects key"),
        Some(projects) => Ok(!projects.contains_key(&crate_name.into())),
    }
}

#[derive(Template)]
#[template(path = "build.gradle", escape = "none")]
struct BuildGradle {
    crate_name: String,
}

const ANDROID_MANIFEST: &str =
    "<manifest xmlns:android=\"http://schemas.android.com/apk/res/android\"/>\n";
const PROGUARD_RULES: &str = "\
# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile
";

#[derive(Template)]
#[template(path = "buildconfig.android.fragment", escape = "none")]
struct BuildConfigFragementTemplate {
    crate_name: String,
    android_root: String,
    description: String,
}

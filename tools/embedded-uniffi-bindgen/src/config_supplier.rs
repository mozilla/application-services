/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// This is the only significant difference from vanilla UniFFI,
//
// We define our own config supplier than parses `Cargo.toml` files directly to avoid a dependency
// on `cargo`.  This code tries to parse as little as possible to make this work.
//
// We could move this code into the main uniffi repo and add a flag to use it instead of the
// cargo-based config supplier.

use std::{
    collections::{HashMap, HashSet},
    env, fs,
    sync::LazyLock,
};

use anyhow::{anyhow, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;
use uniffi_bindgen::BindgenPathsLayer;

pub struct NoCargoConfigSupplier;

impl BindgenPathsLayer for NoCargoConfigSupplier {
    fn get_udl_path(&self, crate_name: &str, udl_name: &str) -> Option<Utf8PathBuf> {
        let crate_map = CRATE_MAP.as_ref().expect("Error parsing Cargo.toml files");
        let crate_root = crate_map.get(crate_name)?;
        Some(crate_root.join(format!("src/{udl_name}.udl")))
    }
}

static CRATE_MAP: LazyLock<Result<HashMap<String, Utf8PathBuf>>> =
    LazyLock::new(find_workspace_crates);

/// Find all crates in this workspace and return a map of crate_name => crate_root_path
fn find_workspace_crates() -> Result<HashMap<String, Utf8PathBuf>> {
    let workspace_toml = find_workspace_toml()?;

    let mut toml_paths_to_process = vec![];
    for path in workspace_toml.data.workspace.unwrap().members {
        toml_paths_to_process.extend(join_and_glob(&workspace_toml.dir, path)?)
    }
    let mut toml_paths_processed = HashSet::new();
    let mut map = HashMap::new();

    loop {
        let Some(crate_dir) = toml_paths_to_process.pop() else {
            break;
        };
        let toml_path = join(&crate_dir, "Cargo.toml")?;
        if !toml_paths_processed.insert(toml_path.clone()) {
            continue;
        }

        let toml = CargoToml::from_path(&toml_path)?;
        let new_paths = find_other_cargo_toml_paths(&crate_dir, &toml);
        toml_paths_to_process.extend(new_paths);

        // Add both the package name and library name to the map
        if let Some(package) = toml.package {
            map.insert(package.name.replace("-", "_"), crate_dir.clone());
        }

        if let Some(CargoLibrary { name: Some(name) }) = toml.lib {
            map.insert(name.replace("-", "_"), crate_dir);
        }
    }
    Ok(map)
}

/// Find the workspace Cargo.toml file.
///
/// Returns the parsed TOML data plus the directory of the file
fn find_workspace_toml() -> Result<CargoTomlFile> {
    let current_dir = camino::Utf8PathBuf::from_path_buf(env::current_dir()?)
        .map_err(|_| anyhow!("path is not UTF8"))?;
    let mut dir = current_dir.as_path();
    loop {
        let path = dir.join("Cargo.toml");
        if path.exists() {
            let toml = CargoToml::from_path(&path)?;
            if toml.workspace.is_some() {
                return Ok(CargoTomlFile {
                    data: toml,
                    dir: dir.to_path_buf(),
                });
            }
        }
        dir = dir
            .parent()
            .ok_or_else(|| anyhow!("Couldn't find workspace Cargo.toml"))?;
    }
}

/// Process Cargo.toml data and return all crate paths referenced in it
fn find_other_cargo_toml_paths(crate_dir: &Utf8Path, toml: &CargoToml) -> Vec<Utf8PathBuf> {
    toml.dependencies
        .iter()
        .flat_map(|d| d.values())
        .filter_map(|dep| match dep {
            // for servo in particular, Cargo.toml specifies relative paths which do not exist, presumably gated by features.
            // eg, `servo/components/style_traits/Cargo.toml` references `../atoms`.
            // So these are just ignored rather than treated as an error.
            CargoDependency::Detailed { path: Some(path) } => join(crate_dir, path).ok(),
            _ => None,
        })
        .collect()
}

fn join(dir: &Utf8Path, child: impl AsRef<str>) -> Result<Utf8PathBuf> {
    let child = child.as_ref();
    dir.join(child)
        .canonicalize_utf8()
        .map_err(|p| anyhow!("Invalid path: {p} {dir}, {child}"))
}

fn join_and_glob(dir: &Utf8Path, child: impl AsRef<str>) -> Result<Vec<Utf8PathBuf>> {
    let child = child.as_ref();
    glob::glob(dir.join(child).as_str())?
        .map(|entry| anyhow::Ok(Utf8PathBuf::try_from(entry?)?))
        .map(|path| Ok(path?.canonicalize_utf8()?))
        .collect()
}

#[derive(Debug)]
struct CargoTomlFile {
    data: CargoToml,
    dir: Utf8PathBuf,
}

#[derive(Debug, Deserialize)]
struct CargoToml {
    package: Option<CargoPackage>,
    lib: Option<CargoLibrary>,
    workspace: Option<CargoWorkspace>,
    dependencies: Option<HashMap<String, CargoDependency>>,
}

impl CargoToml {
    fn from_path(path: &Utf8Path) -> Result<Self> {
        let contents = fs::read_to_string(path).context(format!("reading {path}"))?;
        toml::from_str(&contents).context(format!("parsing {path}"))
    }
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CargoLibrary {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CargoWorkspace {
    members: Vec<Utf8PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CargoDependency {
    #[allow(dead_code)]
    Simple(String),
    Detailed {
        path: Option<Utf8PathBuf>,
    },
}

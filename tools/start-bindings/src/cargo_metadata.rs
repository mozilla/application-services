/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use anyhow::{bail, Result};
use camino::Utf8PathBuf;
use cargo_metadata::{Metadata, MetadataCommand};

pub struct CargoMetadataInfo {
    pub workspace_root: Utf8PathBuf,
    pub crate_root: Utf8PathBuf,
    pub android_megazord_root: Utf8PathBuf,
    pub ios_megazord_root: Utf8PathBuf,
    pub ios_focus_megazord_root: Utf8PathBuf,
}

impl CargoMetadataInfo {
    pub fn new(crate_name: &str) -> Result<Self> {
        let metadata = MetadataCommand::new().exec().unwrap();
        Ok(Self {
            crate_root: find_crate_root(&metadata, crate_name)?,
            android_megazord_root: find_crate_root(&metadata, "megazord")?,
            ios_megazord_root: find_crate_root(&metadata, "megazord_ios")?,
            ios_focus_megazord_root: find_crate_root(&metadata, "megazord_focus")?,
            workspace_root: metadata.workspace_root,
        })
    }
}

fn find_crate_root(metadata: &Metadata, crate_name: &str) -> Result<Utf8PathBuf> {
    let package = metadata.packages.iter().find(|pkg| pkg.name == crate_name);
    match package {
        Some(pkg) => Ok(pkg.manifest_path.parent().unwrap().to_owned()),
        None => bail!("Crate not found: {crate_name}"),
    }
}

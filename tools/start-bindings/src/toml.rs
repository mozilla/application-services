/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::fs::{read_to_string, File};
use std::io::Write;

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use toml_edit::{DocumentMut, Table, Value};

/// A toml file that we're editing
///
/// This wraps toml_edit's DocumentMut for a particular file path
pub struct TomlFile {
    path: Utf8PathBuf,
    doc: DocumentMut,
}

impl TomlFile {
    pub fn open(path: &Utf8Path) -> Result<Self> {
        let doc = if path.exists() {
            read_to_string(path)?.parse()?
        } else {
            DocumentMut::new()
        };
        Ok(Self {
            path: path.to_owned(),
            doc,
        })
    }

    pub fn write(&self) -> Result<()> {
        let mut file = File::create(&self.path)?;
        write!(file, "{}", self.doc)?;
        println!("{} updated", self.path);
        Ok(())
    }
}

impl std::ops::Deref for TomlFile {
    type Target = DocumentMut;

    fn deref(&self) -> &DocumentMut {
        &self.doc
    }
}

impl std::ops::DerefMut for TomlFile {
    fn deref_mut(&mut self) -> &mut DocumentMut {
        &mut self.doc
    }
}

pub fn add_cargo_toml_dependency(
    megazord_root: &Utf8Path,
    crate_root: &Utf8Path,
    crate_name: &str,
) -> Result<()> {
    // Find the relative path from the megazord to the crate root
    let megazord_components: Vec<_> = megazord_root.components().collect();
    let crate_root_components: Vec<_> = crate_root.components().collect();
    let mut i = 0;
    while i < megazord_components.len()
        && i < crate_root_components.len()
        && megazord_components[i] == crate_root_components[i]
    {
        i += 1;
    }
    let mut relpath_components = vec![".."; megazord_components.len() - i];
    for component in crate_root_components.iter().skip(i) {
        relpath_components.push(component.as_str());
    }

    let mut toml = TomlFile::open(&megazord_root.join("Cargo.toml"))?;
    toml["dependencies"][crate_name]["path"] = relpath_components.join("/").into();
    toml.write()
}

pub fn update_uniffi_toml<const N: usize>(
    crate_root: &Utf8Path,
    bindings_name: &str,
    values: [(&str, Value); N],
) -> Result<()> {
    let mut toml = TomlFile::open(&crate_root.join("uniffi.toml"))?;
    if !toml.contains_key("bindings") {
        let mut table = Table::new();
        table.set_implicit(true);
        toml["bindings"] = toml_edit::Item::Table(table);
    }
    toml["bindings"][bindings_name] = toml_edit::Item::Table(Table::from_iter(values));
    toml.write()
}

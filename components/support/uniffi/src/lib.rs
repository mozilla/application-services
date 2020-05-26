/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::io::prelude::*;
use std::{
    env,
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::Result;
use weedle::Parse;

#[derive(Debug)]
struct Definition {
    what: String,
}

pub fn generate_component_stubs(idl_file: &str) {
    println!("cargo:rerun-if-changed={}", idl_file);
    let parsed = parse(idl_file);
    // XXX TODO: give the output file a unique name related to the input file.
    let mut filename = Path::new(idl_file).file_stem().unwrap().to_os_string();
    filename.push(".uniffi.rs");
    let mut out_file = PathBuf::from(env::var("OUT_DIR").unwrap());
    out_file.push(filename);
    let mut f = File::create(out_file).unwrap();
    write!(f, "{:?}", parsed).unwrap();
}

fn parse(idl_file: &str) -> Result<Definition> {
    let mut idl = String::new();
    let mut f = File::open(idl_file)?;
    f.read_to_string(&mut idl)?;
    // XXX TODO: I think the error here needs a lifetime greater than `idl`; unwrap() it for now.
    let (remaining, parsed) = weedle::Definitions::parse(&idl.trim()).unwrap();
    let result = format!("{:?}=-=-=-=-=-=-=-=-=-=-={:?}", parsed, remaining);
    Ok(Definition{what: result})
}

// Notes:
//
// enums -> enums, but they only have names (and in JS they would be string values).
// "value types" -> dictionaries, which would be records in the wit spec.
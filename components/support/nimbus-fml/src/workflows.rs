/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::backends;

use crate::error::Result;
use crate::intermediate_representation::FeatureManifest;
use crate::parser::Parser;
use crate::Config;
use std::fs::File;
use std::path::{Path, PathBuf};

use crate::GenerateStructCmd;

pub(crate) fn generate_struct(config: Option<PathBuf>, cmd: GenerateStructCmd) -> Result<()> {
    let _config = if let Some(path) = config {
        Some(slurp_config(&path)?)
    } else {
        None
    };

    let ir = if cmd.load_from_ir {
        let file = File::open(cmd.manifest)?;
        let _parser: Parser = Parser::new(file);
        unimplemented!("No parser is available")
    } else {
        let string = slurp_file(&cmd.manifest)?;
        serde_json::from_str::<FeatureManifest>(&string)?
    };

    let language = cmd.language;
    match language {
        crate::TargetLanguage::IR => {
            let contents = serde_json::to_string_pretty(&ir)?;
            std::fs::write(cmd.output, contents)?;
        }
        _ => backends::generate_struct(_config, cmd),
    };
    Ok(())
}

fn slurp_config(path: &Path) -> Result<Config> {
    let string = std::fs::read_to_string(path)?;
    Ok(serde_yaml::from_str::<Config>(&string)?)
}

fn slurp_file(path: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(path)?)
}

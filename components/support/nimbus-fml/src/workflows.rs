/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{backends, TargetLanguage};

use crate::error::Result;
use crate::intermediate_representation::FeatureManifest;
use crate::parser::Parser;
use crate::Config;
use std::fs::File;
use std::path::{Path, PathBuf};

use crate::GenerateStructCmd;

pub(crate) fn generate_struct(config: Option<PathBuf>, cmd: GenerateStructCmd) -> Result<()> {
    let config = if let Some(path) = config {
        Some(slurp_config(&path)?)
    } else {
        None
    };

    let ir = if !cmd.load_from_ir {
        let file = File::open(cmd.manifest)?;
        let _parser: Parser = Parser::new(file);
        unimplemented!("No parser is available")
    } else {
        let string = slurp_file(&cmd.manifest)?;
        serde_json::from_str::<FeatureManifest>(&string)?
    };

    let language = cmd.language;
    match language {
        TargetLanguage::IR => {
            let contents = serde_json::to_string_pretty(&ir)?;
            std::fs::write(cmd.output, contents)?;
        }
        TargetLanguage::Kotlin => backends::kotlin::generate_struct(ir, config, cmd)?,
        _ => unimplemented!(),
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

#[cfg(test)]
mod test {

    use std::convert::TryInto;
    use std::fs;

    use anyhow::anyhow;

    use super::*;
    use crate::backends::kotlin;
    use crate::util::{generated_src_dir, join, pkg_dir};

    // Given a manifest.fml and script.kts in the tests directory generate
    // a manifest.kt and run the script against it.
    #[allow(dead_code)]
    fn generate_and_assert(test_script: &str, manifest: &str, is_ir: bool) -> Result<()> {
        let test_script = join(pkg_dir(), test_script);
        let pbuf = PathBuf::from(&test_script);
        let ext = pbuf
            .extension()
            .ok_or_else(|| anyhow!("Require a test_script with an extension: {}", test_script))?;
        let language: TargetLanguage = ext.try_into()?;

        let manifest_fml = join(pkg_dir(), manifest);

        let manifest = PathBuf::from(&manifest_fml);
        let file = manifest
            .file_stem()
            .ok_or_else(|| anyhow!("Manifest file path isn't a file"))?
            .to_str()
            .ok_or_else(|| anyhow!("Manifest file path isn't a file with a sensible name"))?;

        fs::create_dir_all(generated_src_dir())?;

        let manifest_kt = format!(
            "{}.{}",
            join(generated_src_dir(), file),
            language.extension()
        );
        let cmd = GenerateStructCmd {
            manifest: manifest_fml.into(),
            output: manifest_kt.clone().into(),
            load_from_ir: is_ir,
            language,
        };
        generate_struct(None, cmd)?;
        run_script_with_generated_code(language, manifest_kt, &test_script)?;
        Ok(())
    }

    fn run_script_with_generated_code(
        language: TargetLanguage,
        manifest_kt: String,
        test_script: &str,
    ) -> Result<()> {
        match language {
            TargetLanguage::Kotlin => {
                kotlin::test::run_script_with_generated_code(manifest_kt, test_script)?
            }
            _ => unimplemented!(),
        }
        Ok(())
    }

    #[test]
    fn test_simple_validation_code() -> Result<()> {
        generate_and_assert(
            "test/nimbus_validation.kts",
            "fixtures/ir/simple_nimbus_validation.json",
            true,
        )?;
        Ok(())
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{backends, GenerateExperimenterManifestCmd, GenerateIRCmd, TargetLanguage};

use crate::error::Result;
use crate::intermediate_representation::FeatureManifest;
use crate::parser::Parser;
use crate::Config;
use std::path::Path;

use crate::GenerateStructCmd;

pub(crate) fn generate_struct(config: Config, cmd: GenerateStructCmd) -> Result<()> {
    let ir = load_feature_manifest(&cmd.manifest, cmd.load_from_ir, &cmd.channel)?;
    let language = cmd.language;
    match language {
        TargetLanguage::IR => {
            let contents = serde_json::to_string_pretty(&ir)?;
            std::fs::write(cmd.output, contents)?;
        }
        TargetLanguage::Kotlin => backends::kotlin::generate_struct(ir, config, cmd)?,
        TargetLanguage::Swift => backends::swift::generate_struct(ir, config, cmd)?
    };
    Ok(())
}

pub(crate) fn generate_experimenter_manifest(
    config: Config,
    cmd: GenerateExperimenterManifestCmd,
) -> Result<()> {
    let ir = load_feature_manifest(&cmd.manifest, cmd.load_from_ir, &cmd.channel)?;
    backends::experimenter_manifest::generate_manifest(ir, config, cmd)?;
    Ok(())
}

pub(crate) fn generate_ir(_config: Config, cmd: GenerateIRCmd) -> Result<()> {
    let ir = load_feature_manifest(&cmd.manifest, cmd.load_from_ir, &cmd.channel)?;
    std::fs::write(cmd.output, serde_json::to_string_pretty(&ir)?)?;
    Ok(())
}

fn slurp_file(path: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(path)?)
}

fn load_feature_manifest(
    path: &Path,
    load_from_ir: bool,
    channel: &str,
) -> Result<FeatureManifest> {
    let ir = if !load_from_ir {
        let parser: Parser = Parser::new(path, channel)?;
        parser.get_intermediate_representation()?
    } else {
        let string = slurp_file(path)?;
        serde_json::from_str::<FeatureManifest>(&string)?
    };
    ir.validate_manifest()?;
    Ok(ir)
}

#[cfg(test)]
mod test {
    use std::convert::TryInto;
    use std::fs;
    use std::path::PathBuf;

    use anyhow::anyhow;
    use jsonschema::JSONSchema;
    use tempdir::TempDir;

    use super::*;
    use crate::backends::{ kotlin, swift};
    use crate::util::{generated_src_dir, join, pkg_dir};

    const MANIFEST_PATHS: &[&str] = &[
        "fixtures/ir/simple_nimbus_validation.json",
        "fixtures/ir/simple_nimbus_validation.json",
        "fixtures/ir/with_objects.json",
        "fixtures/ir/full_homescreen.json",
    ];

    fn generate_and_assert(
        test_script: &str,
        manifest: &str,
        channel: &str,
        is_ir: bool,
    ) -> Result<()> {
        generate_and_assert_with_config(
            test_script,
            manifest,
            channel,
            is_ir,
            Config {
                resource_package: Some("com.example.app".to_string()),
                ..Default::default()
            },
        )
    }

    // Given a manifest.fml and script.kts in the tests directory generate
    // a manifest.kt and run the script against it.
    fn generate_and_assert_with_config(
        test_script: &str,
        manifest: &str,
        channel: &str,
        is_ir: bool,
        config: Config,
    ) -> Result<()> {
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

        let manifest_out = format!(
            "{}_{}.{}",
            join(generated_src_dir(), file),
            channel,
            language.extension()
        );
        let cmd = GenerateStructCmd {
            manifest: manifest_fml.into(),
            output: manifest_out.clone().into(),
            load_from_ir: is_ir,
            language,
            channel: channel.into(),
        };
        generate_struct(config, cmd)?;
        run_script_with_generated_code(language, manifest_out, &test_script)?;
        Ok(())
    }

    fn run_script_with_generated_code(
        language: TargetLanguage,
        manifest_out: String,
        test_script: &str,
    ) -> Result<()> {
        match language {
            TargetLanguage::Kotlin => {
                kotlin::test::run_script_with_generated_code(manifest_out, test_script)?
            },
            TargetLanguage::Swift => {
                swift::test::run_script_with_generated_code(manifest_out.as_ref(), test_script.as_ref())?
            }
            _ => unimplemented!(),
        }
        Ok(())
    }

    #[test]
    fn test_simple_validation_code() -> Result<()> {
        generate_and_assert(
            "test/simple_nimbus_validation.kts",
            "fixtures/ir/simple_nimbus_validation.json",
            "release",
            true,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_objects_code() -> Result<()> {
        generate_and_assert(
            "test/with_objects.kts",
            "fixtures/ir/with_objects.json",
            "release",
            true,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_full_homescreen() -> Result<()> {
        generate_and_assert(
            "test/full_homescreen.kts",
            "fixtures/ir/full_homescreen.json",
            "release",
            true,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_full_fenix_release() -> Result<()> {
        generate_and_assert_with_config(
            "test/fenix_release.kts",
            "fixtures/fe/fenix.yaml",
            "release",
            false,
            Config {
                resource_package: Some("com.example.app".to_string()),
                nimbus_object_name: Some("FxNimbus".to_string()),
                nimbus_package: Some("com.example.release".to_string()),
            },
        )?;
        Ok(())
    }

    #[test]
    fn test_with_full_fenix_nightly() -> Result<()> {
        generate_and_assert_with_config(
            "test/fenix_nightly.kts",
            "fixtures/fe/fenix.yaml",
            "nightly",
            false,
            Config {
                resource_package: Some("com.example.app".to_string()),
                nimbus_object_name: Some("FxNimbus".to_string()),
                nimbus_package: Some("com.example.nightly".to_string()),
            },
        )?;
        Ok(())
    }

    #[test]
    fn test_with_app_menu() -> Result<()> {
        generate_and_assert(
            "test/app_menu.kts",
            "fixtures/ir/app_menu.json",
            "release",
            true,
        )?;
        Ok(())
    }

    #[test]
    fn smoke_test_ios_generate() -> Result<()> {
        generate_and_assert(
            "test/smoke_test.swift",
            "fixtures/ir/app_menu.json",
            "release",
            true,
        )?;
        Ok(())
    }

    fn validate_against_experimenter_schema<P: AsRef<Path>>(
        schema_path: P,
        generated_json: &serde_json::Value,
    ) -> Result<()> {
        let schema = fs::read_to_string(&schema_path)?;
        let schema: serde_json::Value = serde_json::from_str(&schema)?;
        let compiled = JSONSchema::compile(&schema).expect("The schema is invalid");
        let res = compiled.validate(generated_json);
        if let Err(e) = res {
            let mut errs: String = "Validation errors: \n".into();
            for err in e {
                errs.push_str(&format!("{}\n", err));
            }
            panic!("{}", errs);
        }
        Ok(())
    }

    #[test]
    fn test_schema_validation() -> Result<()> {
        for path in MANIFEST_PATHS {
            let out_tmpdir = TempDir::new("schema_validation").unwrap();
            let manifest_fml = join(pkg_dir(), path);
            let curr_out = out_tmpdir.as_ref().join(path.split('/').last().unwrap());
            let cmd = GenerateExperimenterManifestCmd {
                manifest: manifest_fml.into(),
                output: curr_out.clone(),
                load_from_ir: true,
                channel: "release".into(),
            };
            generate_experimenter_manifest(Default::default(), cmd)?;
            let generated = fs::read_to_string(curr_out)?;
            let generated_json = serde_json::from_str(&generated)?;
            validate_against_experimenter_schema(
                join(pkg_dir(), "ExperimentFeatureManifest.schema.json"),
                &generated_json,
            )?;
        }
        Ok(())
    }
}

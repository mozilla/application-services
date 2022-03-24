/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{backends, GenerateExperimenterManifestCmd, GenerateIRCmd, TargetLanguage};

use crate::error::Result;
use crate::intermediate_representation::FeatureManifest;
use crate::parser::{AboutBlock, Parser};
use std::path::Path;

use crate::GenerateStructCmd;

#[allow(dead_code)]
pub(crate) fn generate_struct(cmd: &GenerateStructCmd) -> Result<()> {
    let ir = load_feature_manifest(&cmd.manifest, cmd.load_from_ir, &cmd.channel)?;
    generate_struct_from_ir(&ir, cmd)
}

pub(crate) fn generate_struct_cli_overrides(
    from_cli: AboutBlock,
    cmd: &GenerateStructCmd,
) -> Result<()> {
    let mut ir = load_feature_manifest(&cmd.manifest, cmd.load_from_ir, &cmd.channel)?;

    // We do a dance here to make sure that we can override class names and package names during tests,
    // and while we still have to support setting those options from the commmand line.
    // We will deprecate setting classnames, package names etc, then we can simplify.
    let from_file = ir.about;
    let from_cli = from_cli;
    let kotlin_about = from_cli.kotlin_about.or(from_file.kotlin_about);
    let swift_about = from_cli.swift_about.or(from_file.swift_about);
    let about = AboutBlock {
        kotlin_about,
        swift_about,
        ..Default::default()
    };
    ir.about = about;

    generate_struct_from_ir(&ir, cmd)
}

fn generate_struct_from_ir(ir: &FeatureManifest, cmd: &GenerateStructCmd) -> Result<()> {
    let language = cmd.language;

    match language {
        TargetLanguage::IR => {
            let contents = serde_json::to_string_pretty(&ir)?;
            std::fs::write(&cmd.output, contents)?;
        }
        TargetLanguage::Kotlin => backends::kotlin::generate_struct(ir, cmd)?,
        TargetLanguage::Swift => backends::swift::generate_struct(ir, cmd)?,
        _ => unimplemented!(
            "Unsupported output language for structs: {}",
            language.extension()
        ),
    };
    Ok(())
}

pub(crate) fn generate_experimenter_manifest(cmd: GenerateExperimenterManifestCmd) -> Result<()> {
    let ir = load_feature_manifest(&cmd.manifest, cmd.load_from_ir, &cmd.channel)?;
    backends::experimenter_manifest::generate_manifest(ir, cmd)?;
    Ok(())
}

pub(crate) fn generate_ir(cmd: GenerateIRCmd) -> Result<()> {
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
        let parser: Parser = Parser::new(path)?;
        parser.get_intermediate_representation(channel)?
    } else {
        let string = slurp_file(path)?;
        serde_json::from_str::<FeatureManifest>(&string)?
    };
    ir.validate_manifest()?;
    Ok(ir)
}

#[cfg(test)]
mod test {
    use std::fs;
    use std::path::PathBuf;

    use anyhow::anyhow;
    use jsonschema::JSONSchema;

    use super::*;
    use crate::backends::{kotlin, swift};
    use crate::parser::KotlinAboutBlock;
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
        let cmd = create_command_from_test(test_script, manifest, channel, is_ir)?;
        generate_struct(&cmd)?;
        run_script_with_generated_code(
            cmd.language,
            cmd.output.as_path().display().to_string(),
            test_script,
        )?;
        Ok(())
    }

    // Given a manifest.fml and script.kts in the tests directory generate
    // a manifest.kt and run the script against it.
    fn generate_and_assert_with_config(
        test_script: &str,
        manifest: &str,
        channel: &str,
        is_ir: bool,
        config_about: AboutBlock,
    ) -> Result<()> {
        let cmd = create_command_from_test(test_script, manifest, channel, is_ir)?;
        generate_struct_cli_overrides(config_about, &cmd)?;
        run_script_with_generated_code(
            cmd.language,
            cmd.output.as_path().display().to_string(),
            test_script,
        )?;
        Ok(())
    }

    fn create_command_from_test(
        test_script: &str,
        manifest: &str,
        channel: &str,
        is_ir: bool,
    ) -> Result<GenerateStructCmd, crate::error::FMLError> {
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
        Ok(GenerateStructCmd {
            manifest: manifest_fml.into(),
            output: manifest_out.into(),
            load_from_ir: is_ir,
            language,
            channel: channel.into(),
        })
    }

    fn run_script_with_generated_code(
        language: TargetLanguage,
        manifest_out: String,
        test_script: &str,
    ) -> Result<()> {
        match language {
            TargetLanguage::Kotlin => {
                kotlin::test::run_script_with_generated_code(manifest_out, test_script)?
            }
            TargetLanguage::Swift => swift::test::run_script_with_generated_code(
                manifest_out.as_ref(),
                test_script.as_ref(),
            )?,
            _ => unimplemented!(),
        }
        Ok(())
    }

    #[test]
    fn test_simple_validation_code_from_ir() -> Result<()> {
        generate_and_assert(
            "test/simple_nimbus_validation.kts",
            "fixtures/ir/simple_nimbus_validation.json",
            "release",
            true,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_objects_code_from_ir() -> Result<()> {
        generate_and_assert(
            "test/with_objects.kts",
            "fixtures/ir/with_objects.json",
            "release",
            true,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_full_homescreen_from_ir() -> Result<()> {
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
            AboutBlock {
                kotlin_about: Some(KotlinAboutBlock {
                    package: "com.example.app".to_string(),
                    class: "com.example.release.FxNimbus".to_string(),
                }),
                swift_about: None,
                ..Default::default()
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
            AboutBlock {
                kotlin_about: Some(KotlinAboutBlock {
                    package: "com.example.app".to_string(),
                    class: "com.example.nightly.FxNimbus".to_string(),
                }),
                swift_about: None,
                ..Default::default()
            },
        )?;
        Ok(())
    }

    #[test]
    fn test_with_dx_improvements() -> Result<()> {
        generate_and_assert(
            "test/dx_improvements_testing.kts",
            "fixtures/fe/dx_improvements.yaml",
            "testing",
            false,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_app_menu_from_ir() -> Result<()> {
        generate_and_assert(
            "test/app_menu.kts",
            "fixtures/ir/app_menu.json",
            "release",
            true,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_app_menu_swift_from_ir() -> Result<()> {
        generate_and_assert(
            "test/app_menu.swift",
            "fixtures/ir/app_menu.json",
            "release",
            true,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_objects_swift_from_ir() -> Result<()> {
        generate_and_assert(
            "test/with_objects.swift",
            "fixtures/ir/with_objects.json",
            "release",
            true,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_bundled_resources_kotlin() -> Result<()> {
        generate_and_assert(
            "test/bundled_resources.kts",
            "fixtures/fe/bundled_resouces.yaml",
            "testing",
            false,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_bundled_resources_swift() -> Result<()> {
        generate_and_assert(
            "test/bundled_resources.swift",
            "fixtures/fe/bundled_resouces.yaml",
            "testing",
            false,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_full_fenix_release_swift() -> Result<()> {
        generate_and_assert(
            "test/fenix_release.swift",
            "fixtures/fe/fenix.yaml",
            "release",
            false,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_full_fenix_nightly_swift() -> Result<()> {
        generate_and_assert(
            "test/fenix_nightly.swift",
            "fixtures/fe/fenix.yaml",
            "nightly",
            false,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_full_firefox_ios() -> Result<()> {
        generate_and_assert(
            "test/firefox_ios_release.swift",
            "fixtures/fe/including/ios.yaml",
            "release",
            false,
        )?;
        Ok(())
    }

    #[test]
    fn regression_test_concurrent_access_of_feature_holder_swift() -> Result<()> {
        generate_and_assert(
            "test/threadsafe_feature_holder.swift",
            "fixtures/fe/fenix.yaml",
            "release",
            false,
        )?;
        Ok(())
    }

    #[test]
    fn regression_test_concurrent_access_of_feature_holder_kts() -> Result<()> {
        generate_and_assert(
            "test/threadsafe_feature_holder.kts",
            "fixtures/fe/fenix.yaml",
            "release",
            false,
        )?;
        Ok(())
    }

    fn validate_against_experimenter_schema<P: AsRef<Path>>(
        schema_path: P,
        generated_yaml: &serde_yaml::Value,
    ) -> Result<()> {
        use crate::backends::experimenter_manifest::ExperimenterManifest;
        let generated_manifest: ExperimenterManifest =
            serde_yaml::from_value(generated_yaml.to_owned())?;
        let generated_json = serde_json::to_value(generated_manifest)?;

        let schema = fs::read_to_string(&schema_path)?;
        let schema: serde_json::Value = serde_json::from_str(&schema)?;
        let compiled = JSONSchema::compile(&schema).expect("The schema is invalid");
        let res = compiled.validate(&generated_json);
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
            let manifest_fml = join(pkg_dir(), path);

            let manifest_fml = PathBuf::from(manifest_fml);
            let file = manifest_fml
                .file_stem()
                .ok_or_else(|| anyhow!("Manifest file path isn't a file"))?
                .to_str()
                .ok_or_else(|| anyhow!("Manifest file path isn't a file with a sensible name"))?;

            fs::create_dir_all(generated_src_dir())?;

            let manifest_out = format!("{}.yaml", join(generated_src_dir(), file),);
            let manifest_out: PathBuf = manifest_out.into();
            let cmd = GenerateExperimenterManifestCmd {
                manifest: manifest_fml,
                output: manifest_out.clone(),
                load_from_ir: true,
                channel: "release".into(),
            };

            generate_experimenter_manifest(cmd)?;

            let generated = fs::read_to_string(manifest_out)?;
            let generated_yaml = serde_yaml::from_str(&generated)?;
            validate_against_experimenter_schema(
                join(pkg_dir(), "ExperimentFeatureManifest.schema.json"),
                &generated_yaml,
            )?;
        }
        Ok(())
    }
}

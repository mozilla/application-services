/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use glob::MatchOptions;

use crate::{
    backends,
    commands::{GenerateExperimenterManifestCmd, GenerateIRCmd, GenerateStructCmd, TargetLanguage},
    error::{FMLError, Result},
    intermediate_representation::FeatureManifest,
    parser::{AboutBlock, Parser},
    util::loaders::{FileLoader, FilePath},
    MATCHING_FML_EXTENSION,
};
use std::path::Path;

#[allow(dead_code)]
pub(crate) fn generate_struct(cmd: &GenerateStructCmd) -> Result<()> {
    let files: FileLoader = TryFrom::try_from(&cmd.loader)?;

    let filename = &cmd.manifest;
    let input = files.file_path(filename)?;

    match (&input, &cmd.output.is_dir()) {
        (FilePath::Remote(_), _) => generate_struct_single(&files, input, cmd),
        (FilePath::Local(file), _) if file.is_file() => generate_struct_single(&files, input, cmd),
        (FilePath::Local(dir), true) if dir.is_dir() => generate_struct_from_dir(&files, cmd, dir),
        (_, true) => generate_struct_from_glob(&files, cmd, filename),
        _ => Err(FMLError::CliError(
            "Cannot generate a single output file from an input directory".to_string(),
        )),
    }
}

fn generate_struct_from_dir(files: &FileLoader, cmd: &GenerateStructCmd, cwd: &Path) -> Result<()> {
    let entries = cwd.read_dir()?;
    for entry in entries.filter_map(Result::ok) {
        let pb = entry.path();
        if pb.is_dir() {
            generate_struct_from_dir(files, cmd, &pb)?;
        } else if let Some(nm) = pb.file_name().map(|s| s.to_str().unwrap_or_default()) {
            if nm.ends_with(MATCHING_FML_EXTENSION) {
                let path = pb.as_path().into();
                generate_struct_single(files, path, cmd)?;
            }
        }
    }
    Ok(())
}

fn generate_struct_from_glob(
    files: &FileLoader,
    cmd: &GenerateStructCmd,
    pattern: &str,
) -> Result<()> {
    use glob::glob_with;
    let entries = glob_with(pattern, MatchOptions::new()).unwrap();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.as_path().into();
        generate_struct_single(files, path, cmd)?;
    }
    Ok(())
}

fn generate_struct_single(
    files: &FileLoader,
    manifest_path: FilePath,
    cmd: &GenerateStructCmd,
) -> Result<()> {
    let ir = load_feature_manifest(files.clone(), manifest_path, cmd.load_from_ir, &cmd.channel)?;
    generate_struct_from_ir(&ir, cmd)
}

pub(crate) fn generate_struct_cli_overrides(
    from_cli: AboutBlock,
    cmd: &GenerateStructCmd,
) -> Result<()> {
    let files: FileLoader = TryFrom::try_from(&cmd.loader)?;
    let path = files.file_path(&cmd.manifest)?;
    let mut ir = load_feature_manifest(files, path, cmd.load_from_ir, &cmd.channel)?;

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
    let language = &cmd.language;
    ir.validate_manifest_for_lang(language)?;
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

pub(crate) fn generate_experimenter_manifest(cmd: &GenerateExperimenterManifestCmd) -> Result<()> {
    let files: FileLoader = TryFrom::try_from(&cmd.loader)?;
    let path = files.file_path(&cmd.manifest)?;
    let ir = load_feature_manifest(files, path, cmd.load_from_ir, &cmd.channel)?;
    backends::experimenter_manifest::generate_manifest(ir, cmd)?;
    Ok(())
}

pub(crate) fn generate_ir(cmd: &GenerateIRCmd) -> Result<()> {
    let files: FileLoader = TryFrom::try_from(&cmd.loader)?;
    let path = files.file_path(&cmd.manifest)?;
    let ir = load_feature_manifest(files, path, cmd.load_from_ir, &cmd.channel)?;
    std::fs::write(&cmd.output, serde_json::to_string_pretty(&ir)?)?;
    Ok(())
}

fn load_feature_manifest(
    files: FileLoader,
    path: FilePath,
    load_from_ir: bool,
    channel: &str,
) -> Result<FeatureManifest> {
    let ir = if !load_from_ir {
        let parser: Parser = Parser::new(files, path)?;
        parser.get_intermediate_representation(channel)?
    } else {
        let string = files.read_to_string(&path)?;
        serde_json::from_str::<FeatureManifest>(&string)?
    };
    ir.validate_manifest()?;
    Ok(ir)
}

pub(crate) fn fetch_file(files: &crate::commands::LoaderConfig, nm: &str) -> Result<()> {
    let files: FileLoader = files.try_into()?;
    let file = files.file_path(nm)?;

    let string = files.read_to_string(&file)?;

    println!("{}", string);
    Ok(())
}

#[cfg(test)]
mod test {
    use std::fs;
    use std::path::PathBuf;

    use anyhow::anyhow;
    use jsonschema::JSONSchema;

    use super::*;
    use crate::backends::experimenter_manifest::ExperimenterManifest;
    use crate::backends::{kotlin, swift};
    use crate::parser::KotlinAboutBlock;
    use crate::util::{generated_src_dir, join, pkg_dir};

    const MANIFEST_PATHS: &[&str] = &[
        "fixtures/ir/simple_nimbus_validation.json",
        "fixtures/ir/simple_nimbus_validation.json",
        "fixtures/ir/with_objects.json",
        "fixtures/ir/full_homescreen.json",
        "fixtures/fe/importing/simple/app.yaml",
        "fixtures/fe/importing/diamond/00-app.yaml",
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
            &cmd.language,
            &[cmd.output.as_path().display().to_string()],
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
            &cmd.language,
            &[cmd.output.as_path().display().to_string()],
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
        let file = PathBuf::from(&manifest_fml);
        let file = file
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
        let loader = Default::default();
        Ok(GenerateStructCmd {
            manifest: manifest_fml,
            output: manifest_out.into(),
            load_from_ir: is_ir,
            language,
            channel: channel.into(),
            loader,
        })
    }

    fn generate_multiple_and_assert(test_script: &str, manifests: &[(&str, &str)]) -> Result<()> {
        let cmds = manifests
            .iter()
            .map(|(manifest, channel)| {
                let cmd = create_command_from_test(test_script, manifest, channel, false)?;
                generate_struct(&cmd)?;
                Ok(cmd)
            })
            .collect::<Result<Vec<_>>>()?;

        let first = cmds
            .first()
            .expect("At least one manifests are always used");
        let language = &first.language;

        let manifests_out = cmds
            .iter()
            .map(|cmd| cmd.output.display().to_string())
            .collect::<Vec<_>>();

        run_script_with_generated_code(language, &manifests_out, test_script)?;
        Ok(())
    }

    fn run_script_with_generated_code(
        language: &TargetLanguage,
        manifests_out: &[String],
        test_script: &str,
    ) -> Result<()> {
        match language {
            TargetLanguage::Kotlin => {
                kotlin::test::run_script_with_generated_code(manifests_out, test_script)?
            }
            TargetLanguage::Swift => {
                swift::test::run_script_with_generated_code(manifests_out, test_script.as_ref())?
            }
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
            "fixtures/fe/browser.yaml",
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
            "fixtures/fe/browser.yaml",
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
            "fixtures/fe/browser.yaml",
            "release",
            false,
        )?;
        Ok(())
    }

    #[test]
    fn test_with_full_fenix_nightly_swift() -> Result<()> {
        generate_and_assert(
            "test/fenix_nightly.swift",
            "fixtures/fe/browser.yaml",
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
    fn test_importing_simple_ios() -> Result<()> {
        generate_multiple_and_assert(
            "test/importing/simple/app_debug.swift",
            &[
                ("fixtures/fe/importing/simple/app.yaml", "debug"),
                ("fixtures/fe/importing/simple/lib.yaml", "debug"),
            ],
        )?;
        Ok(())
    }

    #[test]
    fn test_importing_simple_android() -> Result<()> {
        generate_multiple_and_assert(
            "test/importing/simple/app_debug.kts",
            &[
                ("fixtures/fe/importing/simple/lib.yaml", "debug"),
                ("fixtures/fe/importing/simple/app.yaml", "debug"),
            ],
        )?;
        Ok(())
    }

    #[test]
    fn test_importing_channel_mismatching_android() -> Result<()> {
        generate_multiple_and_assert(
            "test/importing/channels/app_debug.kts",
            &[
                ("fixtures/fe/importing/channels/app.fml.yaml", "app-debug"),
                ("fixtures/fe/importing/channels/lib.fml.yaml", "debug"),
            ],
        )?;
        Ok(())
    }

    #[test]
    fn test_importing_override_defaults_android() -> Result<()> {
        generate_multiple_and_assert(
            "test/importing/overrides/app_debug.kts",
            &[
                ("fixtures/fe/importing/overrides/app.fml.yaml", "debug"),
                ("fixtures/fe/importing/overrides/lib.fml.yaml", "debug"),
            ],
        )?;
        Ok(())
    }

    #[test]
    fn test_importing_override_defaults_ios() -> Result<()> {
        generate_multiple_and_assert(
            "test/importing/overrides/app_debug.swift",
            &[
                ("fixtures/fe/importing/overrides/app.fml.yaml", "debug"),
                ("fixtures/fe/importing/overrides/lib.fml.yaml", "debug"),
            ],
        )?;
        Ok(())
    }

    #[test]
    fn test_importing_override_defaults_coverall_android() -> Result<()> {
        generate_multiple_and_assert(
            "test/importing/overrides-coverall/app_debug.kts",
            &[
                (
                    "fixtures/fe/importing/overrides-coverall/app.fml.yaml",
                    "debug",
                ),
                (
                    "fixtures/fe/importing/overrides-coverall/lib.fml.yaml",
                    "debug",
                ),
            ],
        )?;
        Ok(())
    }

    #[test]
    fn test_importing_diamond_overrides_android() -> Result<()> {
        // In this test, sublib implements a feature.
        // Both lib and app offer some configuration, and both app and lib
        // need to import sublib.
        generate_multiple_and_assert(
            "test/importing/diamond/00-app.kts",
            &[
                ("fixtures/fe/importing/diamond/00-app.yaml", "debug"),
                ("fixtures/fe/importing/diamond/01-lib.yaml", "debug"),
                ("fixtures/fe/importing/diamond/02-sublib.yaml", "debug"),
            ],
        )?;
        Ok(())
    }

    #[test]
    fn test_importing_diamond_overrides_ios() -> Result<()> {
        // In this test, sublib implements a feature.
        // Both lib and app offer some configuration, and both app and lib
        // need to import sublib.
        generate_multiple_and_assert(
            "test/importing/diamond/00-app.swift",
            &[
                ("fixtures/fe/importing/diamond/00-app.yaml", "debug"),
                ("fixtures/fe/importing/diamond/01-lib.yaml", "debug"),
                ("fixtures/fe/importing/diamond/02-sublib.yaml", "debug"),
            ],
        )?;
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_importing_reexporting_features() -> Result<()> {
        // In this test, sublib implements a feature.
        // Both lib and app offer some configuration, but app doesn't need to know
        // that the feature is provided by sublib– where the feature lives
        // is an implementation detail, and should be encapsulated by lib.
        // This is currently not possible, but filed as EXP-2540.
        generate_multiple_and_assert(
            "test/importing/reexporting/00-app.kts",
            &[
                ("fixtures/fe/importing/reexporting/00-app.yaml", "debug"),
                ("fixtures/fe/importing/reexporting/01-lib.yaml", "debug"),
                ("fixtures/fe/importing/reexporting/02-sublib.yaml", "debug"),
            ],
        )?;
        Ok(())
    }

    #[test]
    fn test_importing_including_imports_android() -> Result<()> {
        generate_multiple_and_assert(
            "test/importing/including-imports/app_release.kts",
            &[
                (
                    "fixtures/fe/importing/including-imports/ui.fml.yaml",
                    "none",
                ),
                (
                    "fixtures/fe/importing/including-imports/app.fml.yaml",
                    "release",
                ),
            ],
        )?;
        Ok(())
    }

    #[test]
    fn test_importing_including_imports_ios() -> Result<()> {
        generate_multiple_and_assert(
            "test/importing/including-imports/app_release.swift",
            &[
                (
                    "fixtures/fe/importing/including-imports/ui.fml.yaml",
                    "none",
                ),
                (
                    "fixtures/fe/importing/including-imports/app.fml.yaml",
                    "release",
                ),
            ],
        )?;
        Ok(())
    }

    #[test]
    fn test_importing_simple_experimenter_manifest() -> Result<()> {
        // Both the app and lib files declare features, so we should have an experimenter manifest file with two features.
        let cmd = create_experimenter_manifest_cmd("fixtures/fe/importing/simple/app.yaml")?;
        let files = FileLoader::default()?;
        let path = files.file_path(&cmd.manifest)?;
        let fm = load_feature_manifest(files, path, cmd.load_from_ir, &cmd.channel)?;
        let m: ExperimenterManifest = fm.try_into()?;

        assert!(m.contains_key("homescreen"));
        assert!(m.contains_key("search"));

        Ok(())
    }

    #[test]
    fn regression_test_concurrent_access_of_feature_holder_swift() -> Result<()> {
        generate_and_assert(
            "test/threadsafe_feature_holder.swift",
            "fixtures/fe/browser.yaml",
            "release",
            false,
        )?;
        Ok(())
    }

    #[test]
    fn regression_test_concurrent_access_of_feature_holder_kts() -> Result<()> {
        generate_and_assert(
            "test/threadsafe_feature_holder.kts",
            "fixtures/fe/browser.yaml",
            "release",
            false,
        )?;
        Ok(())
    }

    fn validate_against_experimenter_schema<P: AsRef<Path>>(
        schema_path: P,
        generated_yaml: &serde_yaml::Value,
    ) -> Result<()> {
        let generated_manifest: ExperimenterManifest =
            serde_yaml::from_value(generated_yaml.to_owned())?;
        let generated_json = serde_json::to_value(generated_manifest)?;

        let schema = fs::read_to_string(&schema_path)?;
        let schema: serde_json::Value = serde_json::from_str(&schema)?;
        let compiled = JSONSchema::compile(&schema).expect("The schema is invalid");
        let res = compiled.validate(&generated_json);
        if let Err(e) = res {
            panic!(
                "Validation errors: \n{}",
                e.map(|e| e.to_string()).collect::<Vec<String>>().join("\n")
            );
        }
        Ok(())
    }

    #[test]
    fn test_schema_validation() -> Result<()> {
        for path in MANIFEST_PATHS {
            let cmd = create_experimenter_manifest_cmd(path)?;
            generate_experimenter_manifest(&cmd)?;

            let generated = fs::read_to_string(&cmd.output)?;
            let generated_yaml = serde_yaml::from_str(&generated)?;
            validate_against_experimenter_schema(
                join(pkg_dir(), "ExperimentFeatureManifest.schema.json"),
                &generated_yaml,
            )?;
        }
        Ok(())
    }

    fn create_experimenter_manifest_cmd(path: &str) -> Result<GenerateExperimenterManifestCmd> {
        let manifest = join(pkg_dir(), path);
        let file = Path::new(&manifest);
        let filestem = file
            .file_stem()
            .ok_or_else(|| anyhow!("Manifest file path isn't a file"))?
            .to_str()
            .ok_or_else(|| anyhow!("Manifest file path isn't a file with a sensible name"))?;

        fs::create_dir_all(generated_src_dir())?;

        let output = format!("{}.yaml", join(generated_src_dir(), filestem)).into();
        let load_from_ir = if let Some(ext) = file.extension() {
            TargetLanguage::ExperimenterJSON == ext.try_into()?
        } else {
            false
        };
        let loader = Default::default();
        Ok(GenerateExperimenterManifestCmd {
            manifest,
            output,
            language: TargetLanguage::ExperimenterYAML,
            load_from_ir,
            channel: "release".into(),
            loader,
        })
    }
}

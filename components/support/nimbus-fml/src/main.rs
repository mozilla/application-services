/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[macro_use]
extern crate clap;

mod backends;
mod commands;
mod error;
#[cfg(test)]
#[allow(dead_code)]
mod fixtures;
mod intermediate_representation;
mod parser;
mod util;
mod workflows;

use anyhow::{bail, Result};
use clap::{App, ArgMatches};
use commands::{
    CliCmd, GenerateExperimenterManifestCmd, GenerateIRCmd, GenerateStructCmd, LoaderConfig,
    TargetLanguage,
};
use parser::{AboutBlock, KotlinAboutBlock, SwiftAboutBlock};

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

const RELEASE_CHANNEL: &str = "release";
const SUPPORT_URL_LOADING: bool = false;

fn main() -> Result<()> {
    let cmd = get_command_from_cli(&mut std::env::args_os(), &std::env::current_dir()?)?;
    process_command(&cmd)
}

fn process_command(cmd: &CliCmd) -> Result<()> {
    match cmd {
        CliCmd::DeprecatedGenerate(params, about) => {
            workflows::generate_struct_cli_overrides(about.clone(), params)?
        }
        CliCmd::Generate(params) => workflows::generate_struct(params)?,
        CliCmd::GenerateExperimenter(params) => workflows::generate_experimenter_manifest(params)?,
        CliCmd::GenerateIR(params) => workflows::generate_ir(params)?,
    };
    Ok(())
}

fn get_command_from_cli<I, T>(args: I, cwd: &Path) -> Result<CliCmd>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let yaml = load_yaml!("cli.yaml");
    let matches = App::from_yaml(yaml).get_matches_from(args);

    Ok(match matches.subcommand() {
        // This command is deprecated and will be removed in a future release. and will be removed in a future release.
        ("android", Some(cmd)) => match cmd.subcommand() {
            ("features", Some(cmd)) => {
                let (class, package, rpackage) = (
                    cmd.value_of("class_name"),
                    cmd.value_of("package"),
                    cmd.value_of("r_package"),
                );
                let config = match (class, package, rpackage) {
                    (Some(class_name), Some(class_package), Some(package_id)) => {
                        Some(KotlinAboutBlock {
                            class: format!("{}.{}", class_package, class_name),
                            package: package_id.to_string(),
                        })
                    },
                    (None, None, None) => None,
                    _ => bail!("class_name, package and r_package need to be specified all together on the command line, or not at all"),
                };
                let cmd = create_generate_command_from_cli(&matches, cwd)?;
                match config {
                    Some(_) => CliCmd::DeprecatedGenerate(
                        cmd,
                        AboutBlock {
                            kotlin_about: config,
                            ..Default::default()
                        },
                    ),
                    _ => CliCmd::Generate(cmd),
                }
            }
            _ => unimplemented!(),
        },
        // This command is deprecated and will be removed in a future release. and will be removed in a future release.
        ("ios", Some(cmd)) => match cmd.subcommand() {
            ("features", Some(cmd)) => {
                let config = cmd.value_of("class_name").map(|class| SwiftAboutBlock {
                    class: class.to_string(),
                    module: "Application".to_string(),
                });
                let cmd = create_generate_command_from_cli(&matches, cwd)?;
                match config {
                    Some(_) => CliCmd::DeprecatedGenerate(
                        cmd,
                        AboutBlock {
                            swift_about: config,
                            ..Default::default()
                        },
                    ),
                    _ => CliCmd::Generate(cmd),
                }
            }
            _ => unimplemented!(),
        },
        // This command is deprecated and will be removed in a future release. and will be removed in a future release.
        ("experimenter", _) => CliCmd::GenerateExperimenter(
            create_generate_command_experimenter_from_cli(&matches, cwd)?,
        ),
        // This command is deprecated and will be removed in a future release. and will be removed in a future release.
        ("intermediate-repr", _) => {
            CliCmd::GenerateIR(create_generate_ir_command_from_cli(&matches, cwd)?)
        }
        ("generate", Some(matches)) => {
            CliCmd::Generate(create_generate_command_from_cli(matches, cwd)?)
        }
        ("generate-experimenter", Some(matches)) => CliCmd::GenerateExperimenter(
            create_generate_command_experimenter_from_cli(matches, cwd)?,
        ),
        (word, _) => unimplemented!("Command {} not implemented", word),
    })
}

fn create_generate_ir_command_from_cli(matches: &ArgMatches, cwd: &Path) -> Result<GenerateIRCmd> {
    let manifest = input_file(matches)?;
    let load_from_ir = matches.is_present("ir");
    Ok(GenerateIRCmd {
        manifest,
        output: file_path("output", matches, cwd)?,
        load_from_ir: matches.is_present("ir") || load_from_ir,
        channel: matches
            .value_of("channel")
            .map(str::to_string)
            .unwrap_or_else(|| RELEASE_CHANNEL.into()),
        loader: create_loader(matches, cwd)?,
    })
}

fn create_generate_command_experimenter_from_cli(
    matches: &ArgMatches,
    cwd: &Path,
) -> Result<GenerateExperimenterManifestCmd> {
    let manifest = input_file(matches)?;
    let load_from_ir =
        TargetLanguage::ExperimenterJSON == TargetLanguage::from_extension(&manifest)?;
    let output =
        file_path("output", matches, cwd).or_else(|_| file_path("OUTPUT", matches, cwd))?;
    let language = output.as_path().try_into()?;
    let channel = matches
        .value_of("channel")
        .map(str::to_string)
        .unwrap_or_else(|| RELEASE_CHANNEL.into());
    let loader = create_loader(matches, cwd)?;
    let cmd = GenerateExperimenterManifestCmd {
        manifest,
        output,
        language,
        load_from_ir,
        channel,
        loader,
    };
    Ok(cmd)
}

fn create_generate_command_from_cli(matches: &ArgMatches, cwd: &Path) -> Result<GenerateStructCmd> {
    let manifest = input_file(matches)?;
    let load_from_ir =
        TargetLanguage::ExperimenterJSON == TargetLanguage::from_extension(&manifest)?;
    let output =
        file_path("output", matches, cwd).or_else(|_| file_path("OUTPUT", matches, cwd))?;
    let language = match matches.value_of("language") {
        Some(s) => TargetLanguage::try_from(s)?, // the language from the cli will always be recognized
        None => output.as_path().try_into().map_err(|_| anyhow::anyhow!("Can't infer a target language from the file or directory, so specify a --language flag explicitly"))?,
    };
    let channel = matches
        .value_of("channel")
        .map(str::to_string)
        .expect("A channel should be specified with --channel");
    let loader = create_loader(matches, cwd)?;
    Ok(GenerateStructCmd {
        language,
        manifest,
        output,
        load_from_ir,
        channel,
        loader,
    })
}

fn create_loader(matches: &ArgMatches, cwd: &Path) -> Result<LoaderConfig> {
    let cwd = cwd.to_path_buf();
    let cache_dir = matches
        .value_of("cache-dir")
        .map(|f| cwd.join(f))
        .unwrap_or_else(std::env::temp_dir);

    let files = matches.values_of("repo-file").unwrap_or_default();
    let repo_files = files.into_iter().map(|s| s.to_string()).collect();

    Ok(LoaderConfig {
        cache_dir,
        repo_files,
        cwd,
    })
}

fn input_file(args: &ArgMatches) -> Result<String> {
    args.value_of("INPUT")
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("INPUT file or directory is needed, but not specified"))
}

fn file_path(name: &str, args: &ArgMatches, cwd: &Path) -> Result<PathBuf> {
    let mut abs = cwd.to_path_buf();
    match args.value_of(name) {
        Some(suffix) => {
            abs.push(suffix);
            Ok(abs)
        }
        _ => bail!("A file path is needed for {}", name),
    }
}

#[cfg(test)]
mod cli_tests {
    use std::env;

    use super::*;

    const FML_BIN: &str = "nimbus-fml";
    const TEST_FILE: &str = "fixtures/fe/importing/simple/app.yaml";
    const CACHE_DIR: &str = "./build/cache";
    const REPO_FILE_1: &str = "./repos.versions.json";
    const REPO_FILE_2: &str = "./repos.local.json";

    fn package_dir() -> Result<PathBuf> {
        let string = env::var("CARGO_MANIFEST_DIR")?;
        Ok(PathBuf::try_from(string)?)
    }

    // All these tests just exercise the command line parsing.
    // Each of the tests construct a command struct from the command line, and then
    // test the command struct against the expected values.

    ///////////////////////////////////////////////////////////////////////////
    // In EXP-2560 we changed the command line to support importing and including.
    // We still want to support the old command line, until we have time to update all the apps
    // and the documentation, so we test that we can call them in the same way.
    // The commands in these tests are taken from the apps (`nimbus-fml.sh` in FxiOS and
    // `NimbusGradlePlugin.groovy` in AC).
    #[test]
    fn test_cli_legacy_generate_android_features() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                TEST_FILE,
                "android",
                "features",
                "--output",
                "./Legacy.kt",
                "--channel",
                "channel-test",
                "--package",
                "com.foo.app.nimbus",
                "--classname",
                "FooNimbus",
                "--r-package",
                "com.foo.app",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::DeprecatedGenerate(_, _)));

        if let CliCmd::DeprecatedGenerate(cmd, about) = cmd {
            assert_eq!(cmd.channel, "channel-test");
            assert_eq!(cmd.language, TargetLanguage::Kotlin);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with("Legacy.kt"));
            assert!(cmd.manifest.ends_with(TEST_FILE));

            assert!(about.swift_about.is_none());
            assert!(about.kotlin_about.is_some());

            let about = about.kotlin_about.unwrap();
            assert_eq!(about.class, "com.foo.app.nimbus.FooNimbus");
            assert_eq!(about.package, "com.foo.app");
        }
        Ok(())
    }

    #[test]
    fn test_cli_legacy_generate_ios_features() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                TEST_FILE,
                "-o",
                "./Legacy.swift",
                "ios",
                "features",
                "--classname",
                "FooNimbus",
                "--channel",
                "channel-test",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::DeprecatedGenerate(_, _)));

        if let CliCmd::DeprecatedGenerate(cmd, about) = cmd {
            assert_eq!(cmd.channel, "channel-test");
            assert_eq!(cmd.language, TargetLanguage::Swift);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with("Legacy.swift"));
            assert!(cmd.manifest.ends_with(TEST_FILE));

            assert!(about.swift_about.is_some());
            assert!(about.kotlin_about.is_none());

            let about = about.swift_about.unwrap();
            assert_eq!(about.class, "FooNimbus");
        }
        Ok(())
    }

    ///////////////////////////////////////////////////////////////////////////
    #[test]
    fn test_cli_legacy_experimenter_android() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                TEST_FILE,
                "experimenter",
                "--output",
                ".experimenter.yaml",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::GenerateExperimenter(_)));

        if let CliCmd::GenerateExperimenter(cmd) = cmd {
            assert_eq!(cmd.channel, "release");
            assert_eq!(cmd.language, TargetLanguage::ExperimenterYAML);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with(".experimenter.yaml"));
            assert!(cmd.manifest.ends_with(TEST_FILE));
        }
        Ok(())
    }

    #[test]
    fn test_cli_legacy_experimenter_ios() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                TEST_FILE,
                "-o",
                ".experimenter.yaml",
                "experimenter",
                "--channel",
                "release",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::GenerateExperimenter(_)));

        if let CliCmd::GenerateExperimenter(cmd) = cmd {
            assert_eq!(cmd.channel, "release");
            assert_eq!(cmd.language, TargetLanguage::ExperimenterYAML);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with(".experimenter.yaml"));
            assert!(cmd.manifest.ends_with(TEST_FILE));
        }
        Ok(())
    }

    ///////////////////////////////////////////////////////////////////////////
    #[test]
    fn test_cli_generate_android_features_language_implied() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                "generate",
                "--channel",
                "channel-test",
                TEST_FILE,
                "./Implied.kt",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::Generate(_)));

        if let CliCmd::Generate(cmd) = cmd {
            assert_eq!(cmd.channel, "channel-test");
            assert_eq!(cmd.language, TargetLanguage::Kotlin);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with("Implied.kt"));
            assert!(cmd.manifest.ends_with(TEST_FILE));
        }
        Ok(())
    }

    #[test]
    fn test_cli_generate_ios_features_language_implied() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                "generate",
                "--channel",
                "channel-test",
                TEST_FILE,
                "./Implied.swift",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::Generate(_)));

        if let CliCmd::Generate(cmd) = cmd {
            assert_eq!(cmd.channel, "channel-test");
            assert_eq!(cmd.language, TargetLanguage::Swift);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with("Implied.swift"));
            assert!(cmd.manifest.ends_with(TEST_FILE));
        }
        Ok(())
    }

    ///////////////////////////////////////////////////////////////////////////

    #[test]
    fn test_cli_generate_features_with_remote_flags() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                "generate",
                "--repo-file",
                REPO_FILE_1,
                "--repo-file",
                REPO_FILE_2,
                "--cache-dir",
                CACHE_DIR,
                TEST_FILE,
                "./Implied.swift",
                "--channel",
                "channel-test",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::Generate(_)));

        if let CliCmd::Generate(cmd) = cmd {
            assert_eq!(cmd.channel, "channel-test");
            assert_eq!(cmd.language, TargetLanguage::Swift);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with("Implied.swift"));
            assert!(cmd.manifest.ends_with(TEST_FILE));
        }
        Ok(())
    }

    ///////////////////////////////////////////////////////////////////////////
    #[test]
    fn test_cli_generate_android_features_language_flag() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                "generate",
                "--channel",
                "channel-test",
                "--language",
                "kotlin",
                TEST_FILE,
                "./build/generated",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::Generate(_)));

        if let CliCmd::Generate(cmd) = cmd {
            assert_eq!(cmd.channel, "channel-test");
            assert_eq!(cmd.language, TargetLanguage::Kotlin);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with("build/generated"));
            assert!(cmd.manifest.ends_with(TEST_FILE));
        }
        Ok(())
    }

    #[test]
    fn test_cli_generate_ios_features_language_flag() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                "generate",
                "--channel",
                "channel-test",
                "--language",
                "swift",
                TEST_FILE,
                "./build/generated",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::Generate(_)));

        if let CliCmd::Generate(cmd) = cmd {
            assert_eq!(cmd.channel, "channel-test");
            assert_eq!(cmd.language, TargetLanguage::Swift);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with("build/generated"));
            assert!(cmd.manifest.ends_with(TEST_FILE));
        }
        Ok(())
    }

    ///////////////////////////////////////////////////////////////////////////
    #[test]
    fn test_cli_generate_experimenter_android() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                "generate-experimenter",
                TEST_FILE,
                ".experimenter.yaml",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::GenerateExperimenter(_)));

        if let CliCmd::GenerateExperimenter(cmd) = cmd {
            assert_eq!(cmd.channel, "release");
            assert_eq!(cmd.language, TargetLanguage::ExperimenterYAML);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with(".experimenter.yaml"));
            assert!(cmd.manifest.ends_with(TEST_FILE));
        }
        Ok(())
    }

    #[test]
    fn test_cli_generate_experimenter_ios() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                "generate-experimenter",
                "--channel",
                "test-channel",
                TEST_FILE,
                ".experimenter.yaml",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::GenerateExperimenter(_)));

        if let CliCmd::GenerateExperimenter(cmd) = cmd {
            assert_eq!(cmd.channel, "test-channel");
            assert_eq!(cmd.language, TargetLanguage::ExperimenterYAML);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with(".experimenter.yaml"));
            assert!(cmd.manifest.ends_with(TEST_FILE));
        }
        Ok(())
    }

    #[test]
    fn test_cli_generate_experimenter_with_json() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                "generate-experimenter",
                TEST_FILE,
                ".experimenter.json",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::GenerateExperimenter(_)));

        if let CliCmd::GenerateExperimenter(cmd) = cmd {
            assert_eq!(cmd.channel, "release");
            assert_eq!(cmd.language, TargetLanguage::ExperimenterJSON);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with(".experimenter.json"));
            assert!(cmd.manifest.ends_with(TEST_FILE));
        }
        Ok(())
    }

    #[test]
    fn test_cli_generate_experimenter_with_remote_flags() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                "generate-experimenter",
                "--repo-file",
                REPO_FILE_1,
                "--repo-file",
                REPO_FILE_2,
                "--cache-dir",
                CACHE_DIR,
                TEST_FILE,
                ".experimenter.json",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::GenerateExperimenter(_)));

        if let CliCmd::GenerateExperimenter(cmd) = cmd {
            assert_eq!(cmd.channel, "release");
            assert_eq!(cmd.language, TargetLanguage::ExperimenterJSON);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with(".experimenter.json"));
            assert!(cmd.manifest.ends_with(TEST_FILE));
        }
        Ok(())
    }
}

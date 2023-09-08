/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

pub(crate) mod commands;
mod workflows;

use crate::intermediate_representation::TargetLanguage;
use crate::util::loaders::LoaderConfig;
use anyhow::{bail, Result};
use clap::{App, ArgMatches};
use commands::{
    CliCmd, GenerateExperimenterManifestCmd, GenerateSingleFileManifestCmd, GenerateStructCmd,
    ValidateCmd,
};

use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

const RELEASE_CHANNEL: &str = "release";

pub fn do_main<I, T>(args: I, cwd: &Path) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cmd = get_command_from_cli(args, cwd)?;
    process_command(&cmd)
}

fn process_command(cmd: &CliCmd) -> Result<()> {
    match cmd {
        CliCmd::Generate(params) => workflows::generate_struct(params)?,
        CliCmd::GenerateExperimenter(params) => workflows::generate_experimenter_manifest(params)?,
        CliCmd::GenerateSingleFileManifest(params) => {
            workflows::generate_single_file_manifest(params)?
        }
        CliCmd::FetchFile(files, nm) => workflows::fetch_file(files, nm)?,
        CliCmd::Validate(params) => workflows::validate(params)?,
    };
    Ok(())
}

fn get_command_from_cli<I, T>(args: I, cwd: &Path) -> Result<CliCmd>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let yaml = clap::load_yaml!("cli.yaml");
    let matches = App::from_yaml(yaml).get_matches_from(args);

    Ok(match matches.subcommand() {
        ("generate", Some(matches)) => {
            CliCmd::Generate(create_generate_command_from_cli(matches, cwd)?)
        }
        ("generate-experimenter", Some(matches)) => CliCmd::GenerateExperimenter(
            create_generate_command_experimenter_from_cli(matches, cwd)?,
        ),
        ("fetch", Some(matches)) => {
            CliCmd::FetchFile(create_loader(matches, cwd)?, input_file(matches)?)
        }
        ("single-file", Some(matches)) => {
            CliCmd::GenerateSingleFileManifest(create_single_file_from_cli(matches, cwd)?)
        }
        ("validate", Some(matches)) => {
            CliCmd::Validate(create_validate_command_from_cli(matches, cwd)?)
        }
        (word, _) => unimplemented!("Command {} not implemented", word),
    })
}

fn create_single_file_from_cli(
    matches: &ArgMatches,
    cwd: &Path,
) -> Result<GenerateSingleFileManifestCmd> {
    let manifest = input_file(matches)?;
    let output =
        file_path("output", matches, cwd).or_else(|_| file_path("OUTPUT", matches, cwd))?;
    let channel = matches
        .value_of("channel")
        .map(str::to_string)
        .unwrap_or_else(|| RELEASE_CHANNEL.into());
    let loader = create_loader(matches, cwd)?;
    Ok(GenerateSingleFileManifestCmd {
        manifest,
        output,
        channel,
        loader,
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
    let load_from_ir = matches!(
        TargetLanguage::from_extension(&manifest),
        Ok(TargetLanguage::ExperimenterJSON)
    );
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
        .map(|f| Some(cwd.join(f)))
        .unwrap_or_default();

    let files = matches.values_of("repo-file").unwrap_or_default();
    let repo_files = files.into_iter().map(|s| s.to_string()).collect();

    let manifest = input_file(matches)?;

    let _ref = matches.value_of("ref").map(String::from);

    let mut refs: BTreeMap<_, _> = Default::default();
    match (LoaderConfig::repo_and_path(&manifest), _ref) {
        (Some((repo, _)), Some(ref_)) => refs.insert(repo, ref_),
        _ => None,
    };

    Ok(LoaderConfig {
        cache_dir,
        repo_files,
        cwd,
        refs,
    })
}

fn create_validate_command_from_cli(matches: &ArgMatches, cwd: &Path) -> Result<ValidateCmd> {
    let manifest = input_file(matches)?;
    let loader = create_loader(matches, cwd)?;
    Ok(ValidateCmd { manifest, loader })
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
    const TEST_DIR: &str = "fixtures/fe/importing/including-imports";
    const GENERATED_DIR: &str = "build/cli-test";
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

    #[test]
    fn test_cli_generate_features_for_directory_input() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                "generate",
                "--language",
                "swift",
                "--channel",
                "release",
                TEST_DIR,
                GENERATED_DIR,
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::Generate(_)));

        if let CliCmd::Generate(cmd) = cmd {
            assert_eq!(cmd.channel, "release");
            assert_eq!(cmd.language, TargetLanguage::Swift);
            assert!(!cmd.load_from_ir);
            assert!(cmd.output.ends_with("build/cli-test"));
            assert_eq!(&cmd.manifest, TEST_DIR);
        }
        Ok(())
    }

    ///////////////////////////////////////////////////////////////////////////
    #[test]
    fn test_cli_generate_validate() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli([FML_BIN, "validate", TEST_FILE], &cwd)?;

        assert!(matches!(cmd, CliCmd::Validate(_)));
        assert!(matches!(cmd, CliCmd::Validate(c) if c.manifest.ends_with(TEST_FILE)));
        Ok(())
    }

    ///////////////////////////////////////////////////////////////////////////
    #[test]
    fn test_cli_add_ref_arg() -> Result<()> {
        let cwd = package_dir()?;
        let cmd = get_command_from_cli(
            [
                FML_BIN,
                "generate-experimenter",
                "--ref",
                "my-tag",
                "@foo/bar/baz.fml.yaml",
                "./baz.yaml",
            ],
            &cwd,
        )?;

        assert!(matches!(cmd, CliCmd::GenerateExperimenter(_)));
        assert!(
            matches!(cmd, CliCmd::GenerateExperimenter(c) if c.loader.refs["@foo/bar"] == "my-tag")
        );
        Ok(())
    }
}

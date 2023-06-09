// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod cli;
mod cmd;
mod config;
mod feature_utils;
mod sources;
mod updater;
mod value_utils;

use anyhow::{bail, Result};
use clap::Parser;
use cli::{Cli, CliCommand, ExperimentArgs};
use sources::{ExperimentListSource, ExperimentSource, ManifestSource};
use std::{ffi::OsString, path::PathBuf};

fn main() -> Result<()> {
    let cmds = get_commands_from_cli(std::env::args_os())?;
    for c in cmds {
        let success = cmd::process_cmd(&c)?;
        if !success {
            bail!("Failed");
        }
    }
    updater::check_for_update();
    Ok(())
}

fn get_commands_from_cli<I, T>(args: I) -> Result<Vec<AppCommand>>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    let app = LaunchableApp::try_from(&cli)?;
    let mut commands: Vec<AppCommand> = Default::default();

    commands.push(AppCommand::try_validate(&cli)?);

    if cli.command.should_kill() {
        commands.push(AppCommand::Kill { app: app.clone() });
    }
    if cli.command.should_reset() {
        commands.push(AppCommand::Reset { app });
    }
    commands.push(AppCommand::try_from(&cli)?);

    Ok(commands)
}

#[derive(Clone, Debug, PartialEq)]
enum LaunchableApp {
    Android {
        package_name: String,
        activity_name: String,
        device_id: Option<String>,
        scheme: Option<String>,
    },
    Ios {
        device_id: String,
        app_id: String,
        scheme: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NimbusApp {
    app_name: String,
    channel: String,
}

impl From<&Cli> for NimbusApp {
    fn from(value: &Cli) -> Self {
        Self {
            channel: value.channel.clone(),
            app_name: value.app.clone(),
        }
    }
}

#[derive(Debug, PartialEq)]
enum AppCommand {
    ApplyFile {
        app: LaunchableApp,
        list: ExperimentListSource,
        preserve_nimbus_db: bool,
    },

    CaptureLogs {
        app: LaunchableApp,
        file: PathBuf,
    },

    Enroll {
        app: LaunchableApp,
        params: NimbusApp,
        experiment: ExperimentSource,
        rollouts: Vec<ExperimentSource>,
        branch: String,
        preserve_targeting: bool,
        preserve_bucketing: bool,
        preserve_nimbus_db: bool,
        deeplink: Option<String>,
    },

    FetchList {
        params: NimbusApp,
        list: ExperimentListSource,
        file: PathBuf,
    },

    FetchRecipes {
        params: NimbusApp,
        recipes: Vec<ExperimentSource>,
        file: PathBuf,
    },

    Kill {
        app: LaunchableApp,
    },

    List {
        params: NimbusApp,
        list: ExperimentListSource,
    },

    LogState {
        app: LaunchableApp,
    },

    // No Op, does nothing.
    NoOp,

    Open {
        app: LaunchableApp,
        deeplink: Option<String>,
    },

    Reset {
        app: LaunchableApp,
    },

    TailLogs {
        app: LaunchableApp,
    },

    Unenroll {
        app: LaunchableApp,
    },

    ValidateExperiment {
        params: NimbusApp,
        manifest: ManifestSource,
        experiment: ExperimentSource,
    },
}

impl AppCommand {
    fn try_validate(cli: &Cli) -> Result<Self> {
        let params = cli.into();
        Ok(match &cli.command {
            CliCommand::Enroll {
                no_validate,
                manifest,
                ..
            }
            | CliCommand::TestFeature {
                no_validate,
                manifest,
                ..
            } if !no_validate => {
                let experiment = ExperimentSource::try_from(cli)?;
                let manifest = ManifestSource::try_from(&params, manifest)?;
                AppCommand::ValidateExperiment {
                    params,
                    experiment,
                    manifest,
                }
            }
            CliCommand::Validate { manifest, .. } => {
                let experiment = ExperimentSource::try_from(cli)?;
                let manifest = ManifestSource::try_from(&params, manifest)?;
                AppCommand::ValidateExperiment {
                    params,
                    experiment,
                    manifest,
                }
            }
            _ => Self::NoOp,
        })
    }
}

impl TryFrom<&Cli> for AppCommand {
    type Error = anyhow::Error;

    fn try_from(cli: &Cli) -> Result<Self> {
        let app = LaunchableApp::try_from(cli)?;
        let params = NimbusApp::from(cli);
        Ok(match cli.command.clone() {
            CliCommand::ApplyFile {
                file,
                preserve_nimbus_db,
            } => {
                let list = ExperimentListSource::try_from(file.as_path())?;
                AppCommand::ApplyFile {
                    app,
                    list,
                    preserve_nimbus_db,
                }
            }
            CliCommand::CaptureLogs { file } => AppCommand::CaptureLogs { app, file },
            CliCommand::Enroll {
                branch,
                rollouts,
                preserve_targeting,
                preserve_bucketing,
                preserve_nimbus_db,
                experiment,
                open,
                ..
            } => {
                // Ensure we get the rollouts from the same place we get the experiment from.
                let mut recipes: Vec<ExperimentSource> = Vec::new();
                for r in rollouts {
                    let rollout = ExperimentArgs {
                        experiment: r,
                        ..experiment.clone()
                    };
                    recipes.push(ExperimentSource::try_from(&rollout)?);
                }

                let experiment = ExperimentSource::try_from(cli)?;

                Self::Enroll {
                    app,
                    params,
                    experiment,
                    branch,
                    rollouts: recipes,
                    preserve_targeting,
                    preserve_bucketing,
                    preserve_nimbus_db,
                    deeplink: open.deeplink,
                }
            }
            CliCommand::Fetch {
                file,
                server,
                recipes,
            } => {
                if !server.is_empty() && !recipes.is_empty() {
                    anyhow::bail!("Cannot fetch experiments AND from a server");
                }

                let mut sources = Vec::new();
                if !recipes.is_empty() {
                    for r in recipes {
                        sources.push(ExperimentSource::try_from(r.as_str())?);
                    }
                    AppCommand::FetchRecipes {
                        recipes: sources,
                        file,
                        params,
                    }
                } else {
                    let list = ExperimentListSource::try_from(server.as_str())?;
                    AppCommand::FetchList { list, file, params }
                }
            }
            CliCommand::List { server, file } => {
                if server.is_some() && file.is_some() {
                    bail!("list supports only a file or a server at the same time")
                }
                let list = if file.is_some() {
                    file.unwrap().as_path().try_into()?
                } else {
                    server.unwrap_or_default().as_str().try_into()?
                };
                AppCommand::List { params, list }
            }
            CliCommand::LogState => AppCommand::LogState { app },
            CliCommand::Open { open, .. } => AppCommand::Open {
                app,
                deeplink: open.deeplink,
            },
            CliCommand::ResetApp => AppCommand::Reset { app },
            CliCommand::TailLogs => AppCommand::TailLogs { app },
            CliCommand::TestFeature { files, open, .. } => {
                let experiment = ExperimentSource::try_from(cli)?;
                let first = files
                    .first()
                    .ok_or_else(|| anyhow::Error::msg("Need at least one file to make a branch"))?;
                let branch = feature_utils::slug(first)?;

                Self::Enroll {
                    app,
                    params,
                    experiment,
                    branch,
                    rollouts: Default::default(),
                    deeplink: open.deeplink,
                    preserve_targeting: false,
                    preserve_bucketing: false,
                    preserve_nimbus_db: false,
                }
            }
            CliCommand::Unenroll => AppCommand::Unenroll { app },
            _ => Self::NoOp,
        })
    }
}

impl CliCommand {
    fn should_kill(&self) -> bool {
        match self {
            Self::List { .. }
            | Self::CaptureLogs { .. }
            | Self::TailLogs { .. }
            | Self::Fetch { .. }
            | Self::Validate { .. } => false,
            Self::Open { no_clobber, .. } => !*no_clobber,
            _ => true,
        }
    }

    fn should_reset(&self) -> bool {
        match self {
            Self::Enroll { open, .. }
            | Self::Open { open, .. }
            | Self::TestFeature { open, .. } => open.reset_app,
            _ => false,
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_launchable_app() -> Result<()> {
        fn cli(app: &str, channel: &str) -> Cli {
            Cli {
                app: app.to_string(),
                channel: channel.to_string(),
                device_id: None,
                command: CliCommand::ResetApp,
            }
        }
        fn android(package: &str, activity: &str, scheme: Option<&str>) -> LaunchableApp {
            LaunchableApp::Android {
                package_name: package.to_string(),
                activity_name: activity.to_string(),
                device_id: None,
                scheme: scheme.map(str::to_string),
            }
        }
        fn ios(id: &str, scheme: Option<&str>) -> LaunchableApp {
            LaunchableApp::Ios {
                app_id: id.to_string(),
                device_id: "booted".to_string(),
                scheme: scheme.map(str::to_string),
            }
        }

        // Firefox for Android, a.k.a. fenix
        assert_eq!(
            LaunchableApp::try_from(&cli("fenix", "developer"))?,
            android("org.mozilla.fenix.debug", ".App", Some("fenix-dev"))
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("fenix", "nightly"))?,
            android("org.mozilla.fenix", ".App", Some("fenix-nightly"))
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("fenix", "beta"))?,
            android("org.mozilla.firefox_beta", ".App", Some("fenix-beta"))
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("fenix", "release"))?,
            android("org.mozilla.firefox", ".App", Some("fenix"))
        );

        // Firefox for iOS
        assert_eq!(
            LaunchableApp::try_from(&cli("firefox_ios", "developer"))?,
            ios("org.mozilla.ios.Fennec", Some("fennec"))
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("firefox_ios", "beta"))?,
            ios("org.mozilla.ios.FirefoxBeta", Some("firefox-beta"))
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("firefox_ios", "release"))?,
            ios("org.mozilla.ios.Firefox", Some("firefox-internal"))
        );

        // Focus for Android
        assert_eq!(
            LaunchableApp::try_from(&cli("focus_android", "developer"))?,
            android(
                "org.mozilla.focus.debug",
                "org.mozilla.focus.activity.MainActivity",
                None,
            )
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("focus_android", "nightly"))?,
            android(
                "org.mozilla.focus.nightly",
                "org.mozilla.focus.activity.MainActivity",
                None,
            )
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("focus_android", "beta"))?,
            android(
                "org.mozilla.focus.beta",
                "org.mozilla.focus.activity.MainActivity",
                None,
            )
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("focus_android", "release"))?,
            android(
                "org.mozilla.focus",
                "org.mozilla.focus.activity.MainActivity",
                None,
            )
        );

        Ok(())
    }

    fn fenix() -> LaunchableApp {
        LaunchableApp::Android {
            package_name: "org.mozilla.fenix.debug".to_string(),
            activity_name: ".App".to_string(),
            device_id: None,
            scheme: Some("fenix-dev".to_string()),
        }
    }

    fn fenix_params() -> NimbusApp {
        NimbusApp {
            app_name: "fenix".to_string(),
            channel: "developer".to_string(),
        }
    }

    fn fenix_manifest() -> ManifestSource {
        ManifestSource {
            github_repo: "mozilla-mobile/firefox-android".to_string(),
            ref_: "main".to_string(),
            manifest_file: "@mozilla-mobile/firefox-android/fenix/app/nimbus.fml.yaml".to_string(),
            channel: "developer".to_string(),
        }
    }

    fn experiment(slug: &str) -> ExperimentSource {
        let endpoint = config::api_v6_production_server();
        ExperimentSource::FromApiV6 {
            slug: slug.to_string(),
            endpoint,
        }
    }

    #[test]
    fn test_enroll() -> Result<()> {
        let observed = get_commands_from_cli([
            "nimbus-cli",
            "--app",
            "fenix",
            "--channel",
            "developer",
            "enroll",
            "my-experiment",
            "--branch",
            "my-branch",
            "--no-validate",
        ])?;

        let expected = vec![
            AppCommand::NoOp,
            AppCommand::Kill { app: fenix() },
            AppCommand::Enroll {
                app: fenix(),
                params: fenix_params(),
                experiment: experiment("my-experiment"),
                rollouts: Default::default(),
                branch: "my-branch".to_string(),
                preserve_targeting: false,
                preserve_bucketing: false,
                preserve_nimbus_db: false,
                deeplink: None,
            },
        ];
        assert_eq!(expected, observed);
        Ok(())
    }

    #[test]
    fn test_enroll_with_reset_app() -> Result<()> {
        let observed = get_commands_from_cli([
            "nimbus-cli",
            "--app",
            "fenix",
            "--channel",
            "developer",
            "enroll",
            "my-experiment",
            "--branch",
            "my-branch",
            "--reset-app",
            "--no-validate",
        ])?;

        let expected = vec![
            AppCommand::NoOp,
            AppCommand::Kill { app: fenix() },
            AppCommand::Reset { app: fenix() },
            AppCommand::Enroll {
                app: fenix(),
                params: fenix_params(),
                experiment: experiment("my-experiment"),
                rollouts: Default::default(),
                branch: "my-branch".to_string(),
                preserve_targeting: false,
                preserve_bucketing: false,
                preserve_nimbus_db: false,
                deeplink: None,
            },
        ];
        assert_eq!(expected, observed);
        Ok(())
    }

    #[test]
    fn test_enroll_with_validate() -> Result<()> {
        let observed = get_commands_from_cli([
            "nimbus-cli",
            "--app",
            "fenix",
            "--channel",
            "developer",
            "enroll",
            "my-experiment",
            "--branch",
            "my-branch",
            "--reset-app",
        ])?;

        let expected = vec![
            AppCommand::ValidateExperiment {
                params: fenix_params(),
                manifest: fenix_manifest(),
                experiment: experiment("my-experiment"),
            },
            AppCommand::Kill { app: fenix() },
            AppCommand::Reset { app: fenix() },
            AppCommand::Enroll {
                app: fenix(),
                params: fenix_params(),
                experiment: experiment("my-experiment"),
                rollouts: Default::default(),
                branch: "my-branch".to_string(),
                preserve_targeting: false,
                preserve_bucketing: false,
                preserve_nimbus_db: false,
                deeplink: None,
            },
        ];
        assert_eq!(expected, observed);
        Ok(())
    }

    #[test]
    fn test_validate() -> Result<()> {
        let observed = get_commands_from_cli([
            "nimbus-cli",
            "--app",
            "fenix",
            "--channel",
            "developer",
            "validate",
            "my-experiment",
        ])?;

        let expected = vec![
            AppCommand::ValidateExperiment {
                params: fenix_params(),
                manifest: fenix_manifest(),
                experiment: experiment("my-experiment"),
            },
            AppCommand::NoOp,
        ];
        assert_eq!(expected, observed);

        // With a specific version of the manifest.
        let observed = get_commands_from_cli([
            "nimbus-cli",
            "--app",
            "fenix",
            "--channel",
            "developer",
            "validate",
            "my-experiment",
            "--version",
            "114",
        ])?;

        let expected = vec![
            AppCommand::ValidateExperiment {
                params: fenix_params(),
                manifest: ManifestSource {
                    ref_: "releases_v114".to_string(),
                    ..fenix_manifest()
                },
                experiment: experiment("my-experiment"),
            },
            AppCommand::NoOp,
        ];
        assert_eq!(expected, observed);

        // With a specific version of the manifest, via a ref.
        let observed = get_commands_from_cli([
            "nimbus-cli",
            "--app",
            "fenix",
            "--channel",
            "developer",
            "validate",
            "my-experiment",
            "--ref",
            "my-tag",
        ])?;

        let expected = vec![
            AppCommand::ValidateExperiment {
                params: fenix_params(),
                manifest: ManifestSource {
                    ref_: "my-tag".to_string(),
                    ..fenix_manifest()
                },
                experiment: experiment("my-experiment"),
            },
            AppCommand::NoOp,
        ];
        assert_eq!(expected, observed);

        // With a file on disk
        let observed = get_commands_from_cli([
            "nimbus-cli",
            "--app",
            "fenix",
            "--channel",
            "developer",
            "validate",
            "my-experiment",
            "--manifest",
            "./manifest.fml.yaml",
        ])?;

        let expected = vec![
            AppCommand::ValidateExperiment {
                params: fenix_params(),
                manifest: ManifestSource {
                    manifest_file: "./manifest.fml.yaml".to_string(),
                    ..fenix_manifest()
                },
                experiment: experiment("my-experiment"),
            },
            AppCommand::NoOp,
        ];
        assert_eq!(expected, observed);

        Ok(())
    }

    #[test]
    fn test_test_feature() -> Result<()> {
        fn experiment(feature_id: &str) -> ExperimentSource {
            ExperimentSource::FromFeatureFiles {
                app: fenix_params(),
                feature_id: feature_id.to_string(),
                files: vec!["./my-branch.json".into(), "./my-treatment.json".into()],
            }
        }

        let observed = get_commands_from_cli([
            "nimbus-cli",
            "--app",
            "fenix",
            "--channel",
            "developer",
            "test-feature",
            "my-feature",
            "./my-branch.json",
            "./my-treatment.json",
        ])?;

        let expected = vec![
            AppCommand::ValidateExperiment {
                params: fenix_params(),
                manifest: fenix_manifest(),
                experiment: experiment("my-feature"),
            },
            AppCommand::Kill { app: fenix() },
            AppCommand::Enroll {
                app: fenix(),
                params: fenix_params(),
                experiment: experiment("my-feature"),
                rollouts: Default::default(),
                branch: "my-branch".to_string(),
                preserve_targeting: false,
                preserve_bucketing: false,
                preserve_nimbus_db: false,
                deeplink: None,
            },
        ];
        assert_eq!(expected, observed);

        // With a specific version of the manifest.
        let observed = get_commands_from_cli([
            "nimbus-cli",
            "--app",
            "fenix",
            "--channel",
            "developer",
            "test-feature",
            "my-feature",
            "./my-branch.json",
            "./my-treatment.json",
            "--version",
            "114",
        ])?;

        let expected = vec![
            AppCommand::ValidateExperiment {
                params: fenix_params(),
                manifest: ManifestSource {
                    ref_: "releases_v114".to_string(),
                    ..fenix_manifest()
                },
                experiment: experiment("my-feature"),
            },
            AppCommand::Kill { app: fenix() },
            AppCommand::Enroll {
                app: fenix(),
                params: fenix_params(),
                experiment: experiment("my-feature"),
                rollouts: Default::default(),
                branch: "my-branch".to_string(),
                preserve_targeting: false,
                preserve_bucketing: false,
                preserve_nimbus_db: false,
                deeplink: None,
            },
        ];
        assert_eq!(expected, observed);

        // With a specific version of the manifest, via a ref.
        let observed = get_commands_from_cli([
            "nimbus-cli",
            "--app",
            "fenix",
            "--channel",
            "developer",
            "test-feature",
            "my-feature",
            "./my-branch.json",
            "./my-treatment.json",
            "--ref",
            "my-tag",
        ])?;

        let expected = vec![
            AppCommand::ValidateExperiment {
                params: fenix_params(),
                manifest: ManifestSource {
                    ref_: "my-tag".to_string(),
                    ..fenix_manifest()
                },
                experiment: experiment("my-feature"),
            },
            AppCommand::Kill { app: fenix() },
            AppCommand::Enroll {
                app: fenix(),
                params: fenix_params(),
                experiment: experiment("my-feature"),
                rollouts: Default::default(),
                branch: "my-branch".to_string(),
                preserve_targeting: false,
                preserve_bucketing: false,
                preserve_nimbus_db: false,
                deeplink: None,
            },
        ];
        assert_eq!(expected, observed);

        // With a file on disk
        let observed = get_commands_from_cli([
            "nimbus-cli",
            "--app",
            "fenix",
            "--channel",
            "developer",
            "test-feature",
            "my-feature",
            "./my-branch.json",
            "./my-treatment.json",
            "--manifest",
            "./manifest.fml.yaml",
        ])?;

        let expected = vec![
            AppCommand::ValidateExperiment {
                params: fenix_params(),
                manifest: ManifestSource {
                    manifest_file: "./manifest.fml.yaml".to_string(),
                    ..fenix_manifest()
                },
                experiment: experiment("my-feature"),
            },
            AppCommand::Kill { app: fenix() },
            AppCommand::Enroll {
                app: fenix(),
                params: fenix_params(),
                experiment: experiment("my-feature"),
                rollouts: Default::default(),
                branch: "my-branch".to_string(),
                preserve_targeting: false,
                preserve_bucketing: false,
                preserve_nimbus_db: false,
                deeplink: None,
            },
        ];
        assert_eq!(expected, observed);

        Ok(())
    }
}

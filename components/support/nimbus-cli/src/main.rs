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
use cli::{Cli, CliCommand};
use sources::{ExperimentListSource, ExperimentSource};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

fn main() -> Result<()> {
    let cmds = get_commands_from_cli(std::env::args_os(), &std::env::current_dir()?)?;
    for c in cmds {
        let success = cmd::process_cmd(&c)?;
        if !success {
            bail!("Failed");
        }
    }
    updater::check_for_update();
    Ok(())
}

fn get_commands_from_cli<I, T>(args: I, _cwd: &Path) -> Result<Vec<AppCommand>>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    let app = LaunchableApp::try_from(&cli)?;
    let cmd = AppCommand::try_from(&app, &cli)?;

    let mut commands: Vec<AppCommand> = Default::default();
    if cli.command.should_kill() {
        commands.push(AppCommand::Kill { app: app.clone() });
    }
    if cli.command.should_reset() {
        commands.push(AppCommand::Reset { app });
    }
    commands.push(cmd);
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
}

impl AppCommand {
    fn try_from(app: &LaunchableApp, cli: &Cli) -> Result<Self> {
        let app = app.clone();
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
                file,
                deeplink,
                ..
            } => {
                let experiment = ExperimentSource::try_from(cli)?;
                let mut recipes = Vec::new();
                for r in rollouts {
                    recipes.push(match file.clone() {
                        Some(file) => ExperimentSource::try_from_file(&file, r.as_str())?,
                        _ => ExperimentSource::try_from(r.as_str())?,
                    });
                }

                Self::Enroll {
                    app,
                    params,
                    experiment,
                    branch,
                    rollouts: recipes,
                    preserve_targeting,
                    preserve_bucketing,
                    preserve_nimbus_db,
                    deeplink,
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
            CliCommand::Open { deeplink, .. } => AppCommand::Open { app, deeplink },
            CliCommand::ResetApp => AppCommand::Reset { app },
            CliCommand::TailLogs => AppCommand::TailLogs { app },
            CliCommand::TestFeature {
                files, deeplink, ..
            } => {
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
                    deeplink,
                    preserve_targeting: false,
                    preserve_bucketing: false,
                    preserve_nimbus_db: false,
                }
            }
            CliCommand::Unenroll => AppCommand::Unenroll { app },
        })
    }
}

impl CliCommand {
    fn should_kill(&self) -> bool {
        match self {
            Self::List { .. }
            | Self::CaptureLogs { .. }
            | Self::TailLogs { .. }
            | Self::Fetch { .. } => false,
            Self::Open { no_clobber, .. } => !*no_clobber,
            _ => true,
        }
    }

    fn should_reset(&self) -> bool {
        match self {
            Self::Enroll { reset_app, .. }
            | Self::Open { reset_app, .. }
            | Self::TestFeature { reset_app, .. } => *reset_app,
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
}

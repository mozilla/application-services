// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod cli;
mod cmd;
mod feature_utils;
mod value_utils;

use clap::Parser;
use cli::{Cli, CliCommand};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::{bail, Result};
// use clap::{load_yaml, App, ArgMatches};
fn main() -> Result<()> {
    let cmds = get_commands_from_cli(std::env::args_os(), &std::env::current_dir()?)?;
    for c in cmds {
        let success = cmd::process_cmd(&c)?;
        if !success {
            bail!("Failed");
        }
    }
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
    if cmd.should_kill() {
        commands.push(AppCommand::Kill { app: app.clone() });
    }
    if cmd.should_reset() {
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
    },
    Ios {
        device_id: String,
        app_id: String,
    },
}

impl TryFrom<&Cli> for LaunchableApp {
    type Error = anyhow::Error;
    fn try_from(value: &Cli) -> Result<Self> {
        let app = value.app.as_str();
        let channel = value.channel.as_str();
        let device_id = value.device_id.clone();

        let prefix = match app {
            "fenix" => Some("org.mozilla"),
            "focus_android" => Some("org.mozilla"),
            "firefox_ios" => Some("org.mozilla.ios"),
            "focus_ios" => Some("org.mozilla.ios"),
            _ => None,
        };

        let suffix = match app {
            "fenix" => Some(match channel {
                "developer" => "fenix.debug",
                "nightly" => "fenix",
                "beta" => "firefox_beta",
                "release" => "firefox",
                _ => bail!(format!("Application {} has no channel '{}'. Try one of developer, nightly, beta or release", app, channel)),
            }),
            "focus_android" => Some(match channel {
                "developer" => "focus.debug",
                "nightly" => "focus.nightly",
                "beta" => "focus.beta",
                "release" => "focus",
                _ => bail!(format!("Application {} has no channel '{}'. Try one of developer, nightly, beta or release", app, channel)),
            }),
            "firefox_ios" => Some(match channel {
                "developer" => "Fennec",
                "beta" => "FirefoxBeta",
                "release" => "Firefox",
                _ => bail!(format!("Application {} has no channel '{}'. Try one of developer, beta or release", app, channel)),
            }),
            "focus_ios" => Some(match channel {
                "developer" => "Focus",
                "beta" => "Focus",
                "release" => "Focus",
                _ => bail!(format!("Application {} has no channel '{}'. Try one of developer, beta or release", app, channel)),
            }),
            _ => None,
        };

        Ok(match (app, prefix, suffix) {
            ("fenix", Some(prefix), Some(suffix)) => Self::Android {
                package_name: format!("{}.{}", prefix, suffix),
                activity_name: ".App".to_string(),
                device_id,
            },
            ("focus_android", Some(prefix), Some(suffix)) => Self::Android {
                package_name: format!("{}.{}", prefix, suffix),
                activity_name: "org.mozilla.focus.activity.MainActivity".to_string(),
                device_id,
            },
            ("firefox_ios", Some(prefix), Some(suffix)) => Self::Ios {
                app_id: format!("{}.{}", prefix, suffix),
                device_id: device_id.unwrap_or_else(|| "booted".to_string()),
            },
            ("focus_ios", Some(prefix), Some(suffix)) => Self::Ios {
                app_id: format!("{}.{}", prefix, suffix),
                device_id: device_id.unwrap_or_else(|| "booted".to_string()),
            },
            _ => unimplemented!(),
        })
    }
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
    CaptureLogs {
        app: LaunchableApp,
        file: PathBuf,
    },

    Enroll {
        app: LaunchableApp,
        params: NimbusApp,
        experiment: ExperimentSource,
        branch: String,
        preserve_targeting: bool,
        preserve_bucketing: bool,
        preserve_nimbus_db: bool,
        reset_app: bool,
    },

    Kill {
        app: LaunchableApp,
    },

    List {
        params: NimbusApp,
        list: ExperimentListSource,
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
            CliCommand::CaptureLogs { file } => AppCommand::CaptureLogs {
                app,
                file: file.clone(),
            },
            CliCommand::Enroll {
                experiment,
                branch,
                preserve_targeting,
                preserve_bucketing,
                preserve_nimbus_db,
                reset_app,
                ..
            } => {
                let experiment = ExperimentSource::try_from(experiment.as_str())?;
                Self::Enroll {
                    app,
                    params,
                    experiment,
                    branch,
                    preserve_targeting,
                    preserve_bucketing,
                    preserve_nimbus_db,
                    reset_app,
                }
            }
            CliCommand::List { server } => {
                let list = server.to_owned().unwrap_or_default();
                let list = list.as_str().try_into()?;
                AppCommand::List { params, list }
            }
            CliCommand::ResetApp => AppCommand::Reset { app },
            CliCommand::TailLogs => AppCommand::TailLogs { app },
            CliCommand::TestFeature {
                feature_id,
                files,
                reset_app,
            } => {
                let first = files
                    .first()
                    .ok_or_else(|| anyhow::Error::msg("Need at least one file to make a branch"))?;
                let branch = feature_utils::slug(first)?;
                let experiment = ExperimentSource::FromFeatureFiles {
                    app: params.clone(),
                    feature_id: feature_id.clone(),
                    files: files.clone(),
                };
                Self::Enroll {
                    app,
                    params,
                    experiment,
                    branch,
                    preserve_targeting: false,
                    preserve_bucketing: false,
                    preserve_nimbus_db: false,
                    reset_app,
                }
            }
            CliCommand::Unenroll => AppCommand::Unenroll { app },
        })
    }
}

impl AppCommand {
    fn should_kill(&self) -> bool {
        !matches!(self, AppCommand::List { .. }) && !matches!(self, AppCommand::TailLogs { .. })
    }

    fn should_reset(&self) -> bool {
        if let AppCommand::Enroll {
            reset_app: reset, ..
        } = self
        {
            *reset
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ExperimentListSource {
    FromRemoteSettings { endpoint: String, is_preview: bool },
}

impl ExperimentListSource {
    fn try_from_pair(server: &str, preview: &str) -> Result<Self> {
        let release = Self::release_server();
        let stage = Self::stage_server();
        let is_preview = preview == "preview";

        let endpoint = match server {
            "" | "release" => release,
            "stage" => stage,
            _ => bail!("Only stage or release currently supported"),
        };

        Ok(Self::FromRemoteSettings {
            endpoint,
            is_preview,
        })
    }

    fn release_server() -> String {
        std::env::var("NIMBUS_URL")
            .unwrap_or_else(|_| "https://firefox.settings.services.mozilla.com".to_string())
    }

    fn stage_server() -> String {
        std::env::var("NIMBUS_URL_STAGE")
            .unwrap_or_else(|_| "https://stage.remote-settings.nonprod.webservices.mozgcp.net".to_string())
    }
}

impl TryFrom<&str> for ExperimentListSource {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        let tokens: Vec<&str> = value.splitn(3, '/').collect();
        let tokens = tokens.as_slice();
        Ok(match tokens {
            [""] => Self::try_from_pair("", "")?,
            ["preview"] => Self::try_from_pair("", "preview")?,
            [server] => Self::try_from_pair(server, "")?,
            [server, "preview"] => Self::try_from_pair(server, "preview")?,
            _ => bail!(format!("Can't unpack '{}' into an experiment; try preview, release, stage, or stage/preview", value)),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ExperimentSource {
    FromList {
        slug: String,
        list: ExperimentListSource,
    },
    FromFeatureFiles {
        app: NimbusApp,
        feature_id: String,
        files: Vec<PathBuf>,
    },
}

impl TryFrom<&str> for ExperimentSource {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        let tokens: Vec<&str> = value.splitn(3, '/').collect();
        let tokens = tokens.as_slice();
        Ok(match tokens {
            [slug] => Self::FromList {
                slug: slug.to_string(),
                list: ExperimentListSource::try_from_pair("", "")?,
            },
            ["preview", slug] => Self::FromList {
                slug: slug.to_string(),
                list: ExperimentListSource::try_from_pair("", "preview")?,
            },
            [server, slug] => Self::FromList {
                slug: slug.to_string(),
                list: ExperimentListSource::try_from_pair(server, "")?,
            },
            [server, "preview", slug] => Self::FromList {
                slug: slug.to_string(),
                list: ExperimentListSource::try_from_pair(server, "preview")?,
            },
            _ => bail!(format!(
                "Can't unpack '{}' into an experiment; try preview/SLUG or stage/SLUG, or stage/preview/SLUG",
                value
            )),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_experiment_list_from_pair() -> Result<()> {
        let release = ExperimentListSource::release_server();
        let stage = ExperimentListSource::stage_server();
        assert_eq!(
            ExperimentListSource::try_from_pair("", "")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_pair("", "preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: true
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_pair("release", "")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_pair("release", "preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: true
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_pair("stage", "")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: stage.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_pair("stage", "preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: stage,
                is_preview: true
            }
        );
        assert_eq!(
            ExperimentListSource::try_from_pair("release", "preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release,
                is_preview: true
            }
        );

        assert!(ExperimentListSource::try_from_pair("not-real", "preview").is_err());

        Ok(())
    }

    #[test]
    fn test_experiment_list_from_str() -> Result<()> {
        let release = ExperimentListSource::release_server();
        let stage = ExperimentListSource::stage_server();
        assert_eq!(
            ExperimentListSource::try_from("")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from("release")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from("stage")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: stage.clone(),
                is_preview: false
            }
        );
        assert_eq!(
            ExperimentListSource::try_from("preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release.clone(),
                is_preview: true
            }
        );
        assert_eq!(
            ExperimentListSource::try_from("release/preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: release,
                is_preview: true
            }
        );
        assert_eq!(
            ExperimentListSource::try_from("stage/preview")?,
            ExperimentListSource::FromRemoteSettings {
                endpoint: stage,
                is_preview: true
            }
        );

        assert!(ExperimentListSource::try_from("not-real/preview").is_err());
        assert!(ExperimentListSource::try_from("release/not-real").is_err());

        Ok(())
    }

    #[test]
    fn test_experiment_source_from_str() -> Result<()> {
        let release = ExperimentListSource::try_from("")?;
        let stage = ExperimentListSource::try_from("stage")?;
        let release_preview = ExperimentListSource::try_from("preview")?;
        let stage_preview = ExperimentListSource::try_from("stage/preview")?;
        let slug = "my-slug".to_string();
        assert_eq!(
            ExperimentSource::try_from("my-slug")?,
            ExperimentSource::FromList {
                list: release.clone(),
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from("release/my-slug")?,
            ExperimentSource::FromList {
                list: release,
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from("stage/my-slug")?,
            ExperimentSource::FromList {
                list: stage,
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from("preview/my-slug")?,
            ExperimentSource::FromList {
                list: release_preview.clone(),
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from("release/preview/my-slug")?,
            ExperimentSource::FromList {
                list: release_preview,
                slug: slug.clone()
            }
        );
        assert_eq!(
            ExperimentSource::try_from("stage/preview/my-slug")?,
            ExperimentSource::FromList {
                list: stage_preview,
                slug
            }
        );

        assert!(ExperimentListSource::try_from("not-real/preview/my-slug").is_err());
        assert!(ExperimentListSource::try_from("release/not-real/my-slug").is_err());

        Ok(())
    }

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
        fn android(package: &str, activity: &str) -> LaunchableApp {
            LaunchableApp::Android {
                package_name: package.to_string(),
                activity_name: activity.to_string(),
                device_id: None,
            }
        }
        fn ios(id: &str) -> LaunchableApp {
            LaunchableApp::Ios {
                app_id: id.to_string(),
                device_id: "booted".to_string(),
            }
        }

        // Firefox for Android, a.k.a. fenix
        assert_eq!(
            LaunchableApp::try_from(&cli("fenix", "developer"))?,
            android("org.mozilla.fenix.debug", ".App")
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("fenix", "nightly"))?,
            android("org.mozilla.fenix", ".App")
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("fenix", "beta"))?,
            android("org.mozilla.firefox_beta", ".App")
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("fenix", "release"))?,
            android("org.mozilla.firefox", ".App")
        );

        // Firefox for iOS
        assert_eq!(
            LaunchableApp::try_from(&cli("firefox_ios", "developer"))?,
            ios("org.mozilla.ios.Fennec")
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("firefox_ios", "beta"))?,
            ios("org.mozilla.ios.FirefoxBeta")
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("firefox_ios", "release"))?,
            ios("org.mozilla.ios.Firefox")
        );

        // Focus for Android
        assert_eq!(
            LaunchableApp::try_from(&cli("focus_android", "developer"))?,
            android(
                "org.mozilla.focus.debug",
                "org.mozilla.focus.activity.MainActivity"
            )
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("focus_android", "nightly"))?,
            android(
                "org.mozilla.focus.nightly",
                "org.mozilla.focus.activity.MainActivity"
            )
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("focus_android", "beta"))?,
            android(
                "org.mozilla.focus.beta",
                "org.mozilla.focus.activity.MainActivity"
            )
        );
        assert_eq!(
            LaunchableApp::try_from(&cli("focus_android", "release"))?,
            android(
                "org.mozilla.focus",
                "org.mozilla.focus.activity.MainActivity"
            )
        );

        Ok(())
    }
}

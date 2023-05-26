// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod cli;
mod cmd;
mod feature_utils;
mod updater;
mod value_utils;

use anyhow::{bail, Result};
use clap::Parser;
use cli::{Cli, CliCommand};
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

        // Scheme for deeplinks.
        let scheme = match app {
            "fenix" => Some(match channel {
                // Firefox for Android defines per channel deeplink schemes in the app/build.gradle.
                // e.g. https://github.com/mozilla-mobile/firefox-android/blob/5d18e7ffe2f3e4505ea815d584d20e66ad10f515/fenix/app/build.gradle#L154
                "developer" => "fenix-dev",
                "nightly" => "fenix-nightly",
                "beta" => "fenix-beta",
                "release" => "fenix",
                _ => unreachable!(),
            }),
            "firefox_ios" => Some(match channel {
                // Firefox for iOS uses MOZ_PUBLIC_URL_SCHEME, which is always
                // [`firefox`](https://github.com/mozilla-mobile/firefox-ios/blob/f1acc8a2232a736e65e235b811372ddbf3e802f8/Client/Configuration/Common.xcconfig#L24)
                // and MOZ_INTERNAL_URL_SCHEME which is different per channel.
                // e.g. https://github.com/mozilla-mobile/firefox-ios/blob/f1acc8a2232a736e65e235b811372ddbf3e802f8/Client/Configuration/Firefox.xcconfig#L12
                // From inspection of the code, there are no different uses for the internal vs
                // public, so very useful for launching the specific app on a phone where you
                // have multiple versions installed.
                "developer" => "fennec",
                "beta" => "firefox-beta",
                "release" => "firefox-internal",
                _ => unreachable!(),
            }),
            // Focus for iOS has two, firefox-focus and firefox-klar
            // It's not clear if Focus's channels are configured for this
            "focus_ios" => Some("firefox-focus"),

            // Focus for Android provides no deeplinks.
            _ => None,
        }
        .map(str::to_string);

        Ok(match (app, prefix, suffix) {
            ("fenix", Some(prefix), Some(suffix)) => Self::Android {
                package_name: format!("{}.{}", prefix, suffix),
                activity_name: ".App".to_string(),
                device_id,
                scheme,
            },
            ("focus_android", Some(prefix), Some(suffix)) => Self::Android {
                package_name: format!("{}.{}", prefix, suffix),
                activity_name: "org.mozilla.focus.activity.MainActivity".to_string(),
                device_id,
                scheme,
            },
            ("firefox_ios", Some(prefix), Some(suffix)) => Self::Ios {
                app_id: format!("{}.{}", prefix, suffix),
                device_id: device_id.unwrap_or_else(|| "booted".to_string()),
                scheme,
            },
            ("focus_ios", Some(prefix), Some(suffix)) => Self::Ios {
                app_id: format!("{}.{}", prefix, suffix),
                device_id: device_id.unwrap_or_else(|| "booted".to_string()),
                scheme,
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
                experiment,
                branch,
                rollouts,
                preserve_targeting,
                preserve_bucketing,
                preserve_nimbus_db,
                file,
                deeplink,
                ..
            } => {
                let experiment = match file.clone() {
                    Some(file) => ExperimentSource::try_from_file(&file, &experiment)?,
                    _ => ExperimentSource::try_from(experiment.as_str())?,
                };
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
                feature_id,
                files,
                deeplink,
                ..
            } => {
                let first = files
                    .first()
                    .ok_or_else(|| anyhow::Error::msg("Need at least one file to make a branch"))?;
                let branch = feature_utils::slug(first)?;
                let experiment = ExperimentSource::FromFeatureFiles {
                    app: params.clone(),
                    feature_id,
                    files,
                };
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

#[derive(Clone, Debug, PartialEq)]
enum ExperimentListSource {
    FromRemoteSettings { endpoint: String, is_preview: bool },
    FromFile { file: PathBuf },
}

impl ExperimentListSource {
    fn try_from_pair(server: &str, preview: &str) -> Result<Self> {
        let release = Self::release_server();
        let stage = Self::stage_server();
        let is_preview = preview == "preview";

        let endpoint = match server {
            "" | "release" | "production" | "prod" => release,
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
            .unwrap_or_else(|_| "https://firefox.settings.services.allizom.org".to_string())
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

impl TryFrom<&Path> for ExperimentListSource {
    type Error = anyhow::Error;

    fn try_from(value: &Path) -> Result<Self> {
        Ok(Self::FromFile {
            file: value.to_path_buf(),
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

impl ExperimentSource {
    fn try_from_file(file: &Path, slug: &str) -> Result<Self> {
        Ok(ExperimentSource::FromList {
            slug: slug.to_string(),
            list: file.try_into()?,
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

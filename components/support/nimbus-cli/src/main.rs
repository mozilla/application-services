// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod cli;
mod cmd;
mod value_utils;

use clap::Parser;
use cli::{Cli, CliCommand};
use std::{ffi::OsString, path::Path};

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

#[derive(Clone)]
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
    Enroll {
        app: LaunchableApp,
        params: NimbusApp,
        experiment: ExperimentSource,
        branch: String,
        preserve_targeting: bool,
        reset: bool,
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

    Unenroll {
        app: LaunchableApp,
    },
}

impl AppCommand {
    fn try_from(app: &LaunchableApp, cli: &Cli) -> Result<Self> {
        let app = app.clone();
        let params = NimbusApp::from(cli);
        Ok(match &cli.command {
            CliCommand::Enroll {
                experiment,
                branch,
                preserve_targeting,
                reset,
                ..
            } => {
                let experiment = ExperimentSource::try_from(experiment.as_str())?;
                let branch = branch.to_owned();
                let preserve_targeting = *preserve_targeting;
                let reset = *reset;
                Self::Enroll {
                    app,
                    params,
                    experiment,
                    branch,
                    preserve_targeting,
                    reset,
                }
            }
            CliCommand::List { server } => {
                let list = server.to_owned().unwrap_or_default();
                let list = list.as_str().try_into()?;
                AppCommand::List { params, list }
            }
            CliCommand::ResetApp => AppCommand::Reset { app },
            CliCommand::Unenroll => AppCommand::Unenroll { app },
        })
    }
}

impl AppCommand {
    fn should_kill(&self) -> bool {
        !matches!(self, AppCommand::List { .. })
    }

    fn should_reset(&self) -> bool {
        if let AppCommand::Enroll { reset, .. } = self {
            *reset
        } else {
            false
        }
    }
}

#[derive(Debug)]
enum ExperimentSource {
    FromList {
        slug: String,
        list: ExperimentListSource,
    },
}

#[derive(Debug)]
enum ExperimentListSource {
    FromRemoteSettings { endpoint: String, is_preview: bool },
}

impl ExperimentListSource {
    fn try_from_pair(server: &str, preview: &str) -> Result<Self> {
        let release = std::env::var("NIMBUS_URL")
            .unwrap_or_else(|_| "https://firefox.settings.services.mozilla.com".to_string());
        let stage = std::env::var("NIMBUS_URL_STAGE")
            .unwrap_or_else(|_| "https://settings.stage.mozaws.net".to_string());
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

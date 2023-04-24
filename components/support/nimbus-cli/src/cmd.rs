// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{
    value_utils::{prepare_experiment, try_extract_data_list, try_find_experiment, CliUtils},
    AppCommand, ExperimentListSource, ExperimentSource, LaunchableApp, NimbusApp,
};
use anyhow::{bail, Result};
use console::Term;
use serde_json::{json, Value};
use std::process::Command;

pub(crate) fn process_cmd(cmd: &AppCommand) -> Result<bool> {
    let status = match cmd {
        AppCommand::Enroll {
            app,
            params,
            experiment,
            branch,
            preserve_targeting,
            ..
        } => app.enroll(params, experiment, branch, preserve_targeting)?,
        AppCommand::Kill { app } => app.kill_app()?,
        AppCommand::List { params, list } => list.ls(params)?,
        AppCommand::Reset { app } => app.reset()?,
        AppCommand::Unenroll { app } => app.unenroll_all()?,
    };

    Ok(status)
}

fn prompt(term: &Term, command: &str) -> Result<()> {
    let prompt = term.style().cyan();
    let style = term.style().yellow();
    term.write_line(&format!(
        "{} {}",
        prompt.apply_to("$"),
        style.apply_to(command)
    ))?;
    Ok(())
}

impl LaunchableApp {
    fn exe(&self) -> Result<Command> {
        Ok(match self {
            Self::Android { device_id, .. } => {
                let adb = std::env::var("ADB_PATH").unwrap_or_else(|_| "adb".to_string());
                let mut cmd = Command::new(adb);
                if let Some(id) = device_id {
                    cmd.args(["-s", id]);
                }
                cmd
            }
            Self::Ios { .. } => {
                let xcrun = std::env::var("XCRUN_PATH").unwrap_or_else(|_| "xcrun".to_string());
                let mut cmd = Command::new(xcrun);
                cmd.arg("simctl");
                cmd
            }
        })
    }

    fn kill_app(&self) -> Result<bool> {
        Ok(match self {
            Self::Android { package_name, .. } => self
                .exe()?
                .arg("shell")
                .arg(format!("am force-stop {}", package_name))
                .spawn()?
                .wait()?
                .success(),
            Self::Ios { app_id, device_id } => {
                let _ = self
                    .exe()?
                    .args(["terminate", device_id, app_id])
                    .output()?;
                true
            }
        })
    }

    fn unenroll_all(&self) -> Result<bool> {
        let payload = json! {{ "data": [] }};
        Ok(match self {
            Self::Android { .. } => self.android_start(&payload)?.spawn()?.wait()?.success(),
            Self::Ios { .. } => self.ios_start(&payload)?.spawn()?.wait()?.success(),
        })
    }

    fn reset(&self) -> Result<bool> {
        Ok(match self {
            Self::Android { package_name, .. } => self
                .exe()?
                .arg("shell")
                .arg(format!("pm clear {}", package_name))
                .spawn()?
                .wait()?
                .success(),
            Self::Ios { .. } => {
                let data = self.ios_app_container("data")?;
                let groups = self.ios_app_container("groups")?;
                self.ios_reset(data, groups)?;
                true
            }
        })
    }

    fn enroll(
        &self,
        params: &NimbusApp,
        experiment: &ExperimentSource,
        branch: &str,
        preserve_targeting: &bool,
    ) -> Result<bool> {
        let experiment = Value::try_from(experiment)?;
        let slug = experiment.get_str("slug")?.to_string();

        let term = Term::stdout();
        prompt(
            &term,
            &format!("# Enrolling in '{0}' branch of {1}", branch, &slug),
        )?;

        if params.app_name != experiment.get_str("appName")? {
            bail!(format!("'{}' is not for app {}", slug, params.app_name));
        }
        let experiment = prepare_experiment(
            &experiment,
            &slug,
            &params.channel,
            branch,
            *preserve_targeting,
            false,
        )?;

        let payload = json! {{ "data": [experiment] }};
        Ok(match self {
            Self::Android { .. } => self.android_start(&payload)?.spawn()?.wait()?.success(),
            Self::Ios { .. } => self.ios_start(&payload)?.spawn()?.wait()?.success(),
        })
    }

    fn ios_app_container(&self, container: &str) -> Result<String> {
        if let Self::Ios { app_id, device_id } = self {
            // We need to get the app container directories, and delete them.
            let output = self
                .exe()?
                .args(["get_app_container", device_id, app_id, container])
                .output()
                .expect("Expected an app-container from the simulator");
            let string = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(string)
        } else {
            unreachable!()
        }
    }

    fn ios_reset(&self, data_dir: String, groups_string: String) -> Result<bool> {
        let term = Term::stdout();
        let data_dir = data_dir.trim_end();
        prompt(&term, "# Resetting the app")?;
        prompt(&term, &format!("rm -Rf {}/* 2>/dev/null", data_dir))?;
        let _ = std::fs::remove_dir_all(data_dir);
        let _ = std::fs::create_dir_all(data_dir);
        let lines = groups_string.split('\n');

        for line in lines {
            let words = line.splitn(2, '\t').collect::<Vec<_>>();
            if let [_, dir] = words.as_slice() {
                prompt(&term, &format!("rm -Rf {}/* 2>/dev/null", dir))?;
                let _ = std::fs::remove_dir_all(dir);
                let _ = std::fs::create_dir_all(dir);
            }
        }
        Ok(true)
    }

    fn android_start(&self, json: &Value) -> Result<Command> {
        if let Self::Android {
            package_name,
            activity_name,
            ..
        } = self
        {
            let json = json.to_string().replace('\'', "\\\"");
            let mut cmd = self.exe()?;
            // TODO add adb pass through args for debugger, wait for debugger etc.
            let sh = format!(
                r#"am start -n {}/{} \
        -a android.intent.action.MAIN \
        -c android.intent.category.LAUNCHER \
        --esn nimbus-cli \
        --ei version 1 \
        --es experiments '{}'"#,
                package_name, activity_name, json,
            );
            cmd.arg("shell").arg(&sh);
            let term = Term::stdout();
            prompt(&term, &format!("adb shell \"{}\"", sh))?;
            Ok(cmd)
        } else {
            unreachable!();
        }
    }

    fn ios_start(&self, json: &Value) -> Result<Command> {
        if let Self::Ios { app_id, device_id } = self {
            let mut cmd = self.exe()?;
            cmd.args(["launch", device_id, app_id])
                .arg("--nimbus-cli")
                .args(["--version", "1"])
                .args(["--experiments", &json.to_string()]);

            let sh = format!(
                r#"xcrun simctl launch {} {} \
        --nimbus-cli \
        --version 1 \
        --experiments '{}'"#,
                device_id, app_id, json
            );
            let term = Term::stdout();
            prompt(&term, &sh)?;
            Ok(cmd)
        } else {
            unreachable!()
        }
    }
}

impl TryFrom<&ExperimentSource> for Value {
    type Error = anyhow::Error;

    fn try_from(value: &ExperimentSource) -> Result<Value> {
        Ok(match value {
            ExperimentSource::FromList { slug, list } => {
                let value = Value::try_from(list)?;
                try_find_experiment(&value, slug)?
            }
        })
    }
}

impl TryFrom<&ExperimentListSource> for Value {
    type Error = anyhow::Error;

    fn try_from(value: &ExperimentListSource) -> Result<Value> {
        Ok(match value {
            ExperimentListSource::FromRemoteSettings {
                endpoint,
                is_preview,
            } => {
                use rs_client::{Client, ClientConfig};
                viaduct_reqwest::use_reqwest_backend();
                let collection_name = if *is_preview {
                    "nimbus-preview".to_string()
                } else {
                    "nimbus-mobile-experiments".to_string()
                };
                let config = ClientConfig {
                    server_url: Some(endpoint.clone()),
                    bucket_name: None,
                    collection_name,
                };
                let client = Client::new(config)?;

                let response = client.get_records()?;
                response.json::<Value>()?
            }
        })
    }
}

impl ExperimentListSource {
    fn ls(&self, params: &NimbusApp) -> Result<bool> {
        let value: Value = self.try_into()?;
        let array = try_extract_data_list(&value)?;
        let term = Term::stdout();
        term.write_line(&format!(
            "{0: <65} {1: <30} {2}",
            "Experiment slug", "Features", "Branches"
        ))?;
        for exp in array {
            let slug = exp.get_str("slug")?;
            let app_name = exp.get_str("appName")?;
            if app_name != params.app_name {
                continue;
            }
            let features: Vec<_> = exp
                .get_array("featureIds")?
                .iter()
                .flat_map(|f| f.as_str())
                .collect();
            let branches: Vec<_> = exp
                .get_array("branches")?
                .iter()
                .flat_map(|b| {
                    b.get("slug")
                        .expect("Expecting a branch with a slug")
                        .as_str()
                })
                .collect();

            term.write_line(&format!(
                "{0: <65} {1: <30} {2}",
                slug,
                features.join(", "),
                branches.join(", ")
            ))?;
        }
        Ok(true)
    }
}

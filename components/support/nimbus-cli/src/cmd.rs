// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::borrow::Cow;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use nimbus_fml::intermediate_representation::FeatureManifest;
use serde_json::{Value, json};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::protocol::StartAppProtocol;
use crate::sources::ManifestSource;
use crate::value_utils::{
    self, CliUtils, prepare_experiment, prepare_rollout, try_find_branches_from_experiment,
    try_find_features_from_branch,
};
use crate::{
    AppCommand, AppOpenArgs, ExperimentListSource, ExperimentSource, LaunchableApp, NimbusApp,
};

pub(crate) fn process_cmd(cmd: &AppCommand) -> Result<bool> {
    let status = match cmd {
        AppCommand::ApplyFile {
            app,
            open,
            list,
            preserve_nimbus_db,
        } => app.apply_list(open, list, preserve_nimbus_db)?,
        AppCommand::CaptureLogs { app, file } => app.capture_logs(file)?,
        AppCommand::Defaults {
            manifest,
            feature_id,
            output,
        } => manifest.print_defaults(feature_id.as_ref(), output.as_ref())?,
        AppCommand::Enroll {
            app,
            params,
            experiment,
            rollouts,
            branch,
            preserve_targeting,
            preserve_bucketing,
            preserve_nimbus_db,
            open,
            ..
        } => app.enroll(
            params,
            experiment,
            rollouts,
            branch,
            preserve_targeting,
            preserve_bucketing,
            preserve_nimbus_db,
            open,
        )?,
        AppCommand::ExtractFeatures {
            experiment,
            branch,
            manifest,
            feature_id,
            validate,
            multi,
            output,
        } => experiment.print_features(
            branch,
            manifest,
            feature_id.as_ref(),
            *validate,
            *multi,
            output.as_ref(),
        )?,

        AppCommand::FetchList { list, file } => list.fetch_list(file.as_ref())?,
        AppCommand::FmlPassthrough { cmd } => cmd.dispatch().map(|_| true)?,
        AppCommand::Info { experiment, output } => experiment.print_info(output.as_ref())?,
        AppCommand::Kill { app } => app.kill_app()?,
        AppCommand::List { list, .. } => list.print_list()?,
        AppCommand::LogState { app, open } => app.log_state(open)?,
        AppCommand::NoOp => true,
        AppCommand::Open {
            app, open: args, ..
        } => app.open(args)?,
        AppCommand::Reset { app } => app.reset_app()?,
        AppCommand::TailLogs { app } => app.tail_logs()?,
        AppCommand::Unenroll { app, open } => app.unenroll_all(open)?,
        AppCommand::ValidateExperiment {
            params,
            manifest,
            experiment,
        } => params.validate_experiment(manifest, experiment)?,
        AppCommand::EvalJexl {
            app,
            expression,
            open,
        } => app.eval_jexl(expression, open)?,
    };

    Ok(status)
}

fn write_prompt(stream: &mut StandardStream, command: &str) -> Result<()> {
    stream.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
    write!(stream, "$ ")?;
    stream.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
    writeln!(stream, "{}", command)?;
    stream.reset()?;

    Ok(())
}

fn write_info(stream: &mut StandardStream, text: &str) -> Result<()> {
    stream.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
    write!(stream, "ℹ️  ")?;
    stream.set_color(ColorSpec::new().set_fg(None))?;
    writeln!(stream, "{}", text)?;
    stream.reset()?;

    Ok(())
}

fn write_ok(stream: &mut StandardStream, title: &str) -> Result<()> {
    write!(stream, "✅ ")?;
    stream.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
    writeln!(stream, "{title}")?;
    stream.reset()?;

    Ok(())
}

fn write_err(stream: &mut StandardStream, title: &str, detail: &str) -> Result<()> {
    write!(stream, "❎ ")?;
    stream.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
    write!(stream, "{title}")?;
    stream.reset()?;
    writeln!(stream, ": {detail}")?;

    Ok(())
}

impl LaunchableApp {
    fn exe(&self) -> Result<Command> {
        Ok(match self {
            Self::Android { device_id, .. } => {
                let adb_name = if std::env::consts::OS != "windows" {
                    "adb"
                } else {
                    "adb.exe"
                };
                let adb = std::env::var("ADB_PATH").unwrap_or_else(|_| adb_name.to_string());
                let mut cmd = Command::new(adb);
                if let Some(id) = device_id {
                    cmd.args(["-s", id]);
                }
                cmd
            }
            Self::Ios { .. } => {
                if std::env::consts::OS != "macos" {
                    panic!("Cannot run commands for iOS on anything except macOS");
                }
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
                .args(["shell", "--", "am", "force-stop", &*package_name])
                .spawn()?
                .wait()?
                .success(),
            Self::Ios {
                app_id, device_id, ..
            } => {
                let _ = self
                    .exe()?
                    .args(["terminate", device_id, app_id])
                    .output()?;
                true
            }
        })
    }

    fn unenroll_all(&self, open: &AppOpenArgs) -> Result<bool> {
        let payload = TryFrom::try_from(&ExperimentListSource::Empty)?;
        let protocol = StartAppProtocol {
            log_state: true,
            experiments: Some(&payload),
            ..Default::default()
        };
        self.start_app(protocol, open)
    }

    fn reset_app(&self) -> Result<bool> {
        Ok(match self {
            Self::Android { package_name, .. } => self
                .exe()?
                .args(["shell", "--", "pm", "clear", &*package_name])
                .spawn()?
                .wait()?
                .success(),
            Self::Ios {
                app_id, device_id, ..
            } => {
                self.exe()?
                    .args(["privacy", device_id, "reset", "all", app_id])
                    .status()?;
                let data = self.ios_app_container("data")?;
                let groups = self.ios_app_container("groups")?;
                self.ios_reset(data, groups)?;
                true
            }
        })
    }

    fn tail_logs(&self) -> Result<bool> {
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);

        Ok(match self {
            Self::Android { .. } => {
                let mut args = logcat_args();
                args.append(&mut vec!["-v", "color"]);
                write_prompt(&mut stdout, &format!("adb {}", args.join(" ")))?;
                self.exe()?.args(args).spawn()?.wait()?.success()
            }
            Self::Ios { .. } => {
                write_prompt(
                    &mut stdout,
                    &format!("{} | xargs tail -f", self.ios_log_file_command()),
                )?;
                let log = self.ios_log_file()?;

                Command::new("tail")
                    .arg("-f")
                    .arg(log.as_path().to_str().unwrap())
                    .spawn()?
                    .wait()?
                    .success()
            }
        })
    }

    fn capture_logs(&self, file: &PathBuf) -> Result<bool> {
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);

        Ok(match self {
            Self::Android { .. } => {
                let mut args = logcat_args();
                args.append(&mut vec!["-d"]);
                write_prompt(
                    &mut stdout,
                    &format!(
                        "adb {} > {}",
                        args.join(" "),
                        file.as_path().to_str().unwrap()
                    ),
                )?;
                let output = self.exe()?.args(args).output()?;
                std::fs::write(file, String::from_utf8_lossy(&output.stdout).to_string())?;
                true
            }

            Self::Ios { .. } => {
                let log = self.ios_log_file()?;
                write_prompt(
                    &mut stdout,
                    &format!(
                        "{} | xargs -J %log_file% cp %log_file% {}",
                        self.ios_log_file_command(),
                        file.as_path().to_str().unwrap()
                    ),
                )?;
                std::fs::copy(log, file)?;
                true
            }
        })
    }

    fn ios_log_file(&self) -> Result<PathBuf> {
        let data = self.ios_app_container("data")?;
        let mut files = glob::glob(&format!("{}/**/*.log", data))?;

        files
            .next()
            .transpose()?
            .context("Logs are not available before the app is started for the first time")
    }

    fn ios_log_file_command(&self) -> String {
        if let Self::Ios {
            device_id, app_id, ..
        } = self
        {
            format!(
                "find $(xcrun simctl get_app_container {0} {1} data) -name \\*.log",
                device_id, app_id
            )
        } else {
            unreachable!()
        }
    }

    fn log_state(&self, open: &AppOpenArgs) -> Result<bool> {
        let protocol = StartAppProtocol {
            log_state: true,
            ..Default::default()
        };
        self.start_app(protocol, open)
    }

    #[allow(clippy::too_many_arguments)]
    fn enroll(
        &self,
        params: &NimbusApp,
        experiment: &ExperimentSource,
        rollouts: &Vec<ExperimentSource>,
        branch: &str,
        preserve_targeting: &bool,
        preserve_bucketing: &bool,
        preserve_nimbus_db: &bool,
        open: &AppOpenArgs,
    ) -> Result<bool> {
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);

        let experiment = Value::try_from(experiment)?;
        let slug = experiment.get_str("slug")?.to_string();

        let mut recipes = vec![prepare_experiment(
            &experiment,
            params,
            branch,
            *preserve_targeting,
            *preserve_bucketing,
        )?];
        write_info(
            &mut stdout,
            &format!("Enrolling in the '{0}' branch of '{1}'", branch, slug),
        )?;

        for r in rollouts {
            let rollout = Value::try_from(r)?;
            let slug = rollout.get_str("slug")?.to_string();
            recipes.push(prepare_rollout(
                &rollout,
                params,
                *preserve_targeting,
                *preserve_bucketing,
            )?);
            write_info(
                &mut stdout,
                &format!("# Enrolling into the '{0}' rollout", slug),
            )?;
        }

        let payload = json! {{ "data": recipes }};
        let protocol = StartAppProtocol {
            reset_db: !preserve_nimbus_db,
            experiments: Some(&payload),
            log_state: true,
            jexl_expression: None,
        };
        self.start_app(protocol, open)
    }

    fn apply_list(
        &self,
        open: &AppOpenArgs,
        list: &ExperimentListSource,
        preserve_nimbus_db: &bool,
    ) -> Result<bool> {
        let value: Value = list.try_into()?;

        let protocol = StartAppProtocol {
            reset_db: !preserve_nimbus_db,
            experiments: Some(&value),
            log_state: true,
            jexl_expression: None,
        };
        self.start_app(protocol, open)
    }

    fn ios_app_container(&self, container: &str) -> Result<String> {
        if let Self::Ios {
            app_id, device_id, ..
        } = self
        {
            // We need to get the app container directories, and delete them.
            let output = self
                .exe()?
                .args(["get_app_container", device_id, app_id, container])
                .output()
                .expect("Expected an app-container from the simulator");
            let string = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(string.trim().to_string())
        } else {
            unreachable!()
        }
    }

    fn ios_reset(&self, data_dir: String, groups_string: String) -> Result<bool> {
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        write_info(&mut stdout, "Resetting the app")?;
        if !data_dir.is_empty() {
            write_prompt(&mut stdout, &format!("rm -Rf {}/* 2>/dev/null", data_dir))?;
            let _ = std::fs::remove_dir_all(&data_dir);
            let _ = std::fs::create_dir_all(&data_dir);
        }
        let lines = groups_string.split('\n');

        for line in lines {
            let words = line.splitn(2, '\t').collect::<Vec<_>>();
            if let [_, dir] = words.as_slice()
                && !dir.is_empty()
            {
                write_prompt(&mut stdout, &format!("rm -Rf {}/* 2>/dev/null", dir))?;
                let _ = std::fs::remove_dir_all(dir);
                let _ = std::fs::create_dir_all(dir);
            }
        }
        Ok(true)
    }

    fn open(&self, open: &AppOpenArgs) -> Result<bool> {
        self.start_app(Default::default(), open)
    }

    fn eval_jexl(&self, expression: &str, open: &AppOpenArgs) -> Result<bool> {
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        write_info(
            &mut stdout,
            &format!("Evaluating JEXL expression `{expression}`"),
        )?;

        // Clear logcat before starting to ensure we only read fresh logs
        if matches!(self, Self::Android { .. }) {
            self.exe()?.args(["logcat", "-c"]).output()?;
        }

        let protocol = StartAppProtocol {
            jexl_expression: Some(expression),
            ..Default::default()
        };

        if !self.start_app(protocol, open)? {
            write_err(&mut stdout, "Error", "Failed to launch the application")?;

            return Err(anyhow!("Could not evaluate JEXL expression"));
        }

        write_info(&mut stdout, "Waiting for result...")?;

        const MAX_RETRIES: u32 = 3;
        const SLEEP_DURATION: Duration = Duration::from_secs(3);

        let mut attempt = 1;
        let rsp = loop {
            sleep(SLEEP_DURATION);

            match self.capture_jexl_result()? {
                Some(rsp) => break rsp,
                None => {
                    if attempt >= MAX_RETRIES {
                        bail!("Failed to find JEXL result in logs after {attempt} attempts")
                    }

                    write_info(
                        &mut stdout,
                        &format!("Retry {}/{}...", attempt, MAX_RETRIES - 1),
                    )?;

                    attempt += 1;
                }
            }
        };

        let rsp: serde_json::Value =
            serde_json::from_str(&rsp).context("Could not deserialize JSON response")?;

        let rsp = rsp.as_object().context("Expected object")?;

        let success = rsp
            .get("success")
            .context("Unexpected response: no success key")?
            .as_bool()
            .context("Unexpected response: success is not bool")?;

        if success {
            let result = rsp
                .get("result")
                .context("Unexpected response: no result key for successful evaluation")?;

            if let Some(v) = result.as_bool() {
                write_ok(&mut stdout, &v.to_string())?;
            } else {
                write_info(
                    &mut stdout,
                    &format!(
                        "Did not evaluate to a boolean: {}",
                        serde_json::to_string_pretty(result)?
                    ),
                )?;
            }

            Ok(true)
        } else {
            let error = rsp
                .get("error")
                .context("Unexpected response: no error key for unsuccessful evaluation")?
                .as_str()
                .context("Unexpected response: error is not a string")?;

            write_err(&mut stdout, "Error", error)?;
            Ok(false)
        }
    }

    fn capture_jexl_result(&self) -> Result<Option<String>> {
        let output = match self {
            Self::Android { .. } => {
                self.exe()?
                    .args([
                        "logcat",
                        // Dump logs to stdout and exit.
                        "-d",
                        // Silence the default log filter.
                        "-s",
                        // Dump only logs tagged NimbusTooling at INFO or higher.
                        "NimbusTooling:I",
                    ])
                    .output()?
                    .stdout
            }

            Self::Ios { device_id, .. } => {
                self.exe()?
                    .args([
                        "spawn",
                        device_id,
                        "log",
                        "show",
                        "--predicate",
                        "process == 'Client'",
                        "--last",
                        "10s",
                        "--style",
                        "compact",
                    ])
                    .output()?
                    .stdout
            }
        };
        let logs = String::from_utf8_lossy(&output);

        for line in logs.lines().rev() {
            if let Some((_, json)) = line.split_once("JEXL_RESULT: ") {
                return Ok(Some(json.into()));
            }
        }

        Ok(None)
    }

    fn start_app(&self, app_protocol: StartAppProtocol, open: &AppOpenArgs) -> Result<bool> {
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        if let Some(file) = &open.output {
            let ex = app_protocol.experiments;
            if let Some(contents) = ex {
                value_utils::write_to_file_or_print(Some(file), contents)?;
                write_ok(
                    &mut stdout,
                    &format!(
                        "Written to JSON to file {}",
                        file.to_str().unwrap_or_default()
                    ),
                )?;
            } else {
                write_err(
                    &mut stdout,
                    "No content",
                    &format!("File {} not written", file.to_str().unwrap_or_default()),
                )?;
            }

            return Ok(true);
        }

        Ok(match self {
            Self::Android { .. } => self.android_start(app_protocol)?.spawn()?.wait()?.success(),
            Self::Ios { .. } => self.ios_start(app_protocol)?.spawn()?.wait()?.success(),
        })
    }

    fn android_start(&self, app_protocol: StartAppProtocol) -> Result<Command> {
        if let Self::Android { package_name, .. } = self {
            let mut args = Vec::<Cow<str>>::new();

            let StartAppProtocol {
                reset_db,
                experiments,
                log_state,
                jexl_expression,
            } = app_protocol;

            if log_state || experiments.is_some() || reset_db || jexl_expression.is_some() {
                args.extend(["am", "broadcast"].map(Into::into));
                args.extend([
                    "-p".into(),
                    package_name.into(),
                    "-a".into(),
                    "org.mozilla.fenix.NIMBUS_TOOLING".into(),
                    "-f".into(),
                    // android.content.Intent.FLAG_INCLUDE_STOPPED_PACKAGES
                    "0x20".into(),
                ]);
                args.extend(["--esn", "nimbus-cli", "--ei", "version", "1"].map(Into::into));

                if reset_db {
                    args.extend(["--ez", "reset-db", "true"].map(Into::into));
                }

                if let Some(s) = experiments {
                    let json = s.to_string().replace('\'', "&apos;");
                    args.extend(["--es".into(), "experiments".into(), json.into()]);
                }

                if log_state {
                    args.extend(["--ez", "log-state", "true"].map(Into::into));
                }

                if let Some(expr) = jexl_expression {
                    let escaped = expr.replace('\'', "&apos;");
                    args.extend(["--es", "eval-jexl"].map(Into::into));
                    args.push(escaped.into());
                };
            } else {
                args.extend(["am", "start"].map(Into::into));
                args.push("-n".into());
                args.push(format!("{package_name}/.App").into());
                args.extend(
                    [
                        "-a",
                        "android.intent.action.MAIN",
                        "-c",
                        "android.intent.category.LAUNCHER",
                    ]
                    .map(Into::into),
                );
            }

            let mut stdout = StandardStream::stdout(ColorChoice::Auto);
            write_prompt(
                &mut stdout,
                &format!("adb shell -- {}", args.join(" \\\n        ")),
            )?;

            let mut cmd = self.exe()?;
            cmd.args(["shell", "--"])
                .args(args.iter().map(AsRef::as_ref));

            Ok(cmd)
        } else {
            unreachable!();
        }
    }

    fn ios_start(&self, app_protocol: StartAppProtocol) -> Result<Command> {
        if let Self::Ios {
            app_id, device_id, ..
        } = self
        {
            let mut args = Vec::<Cow<str>>::new();

            args.push("launch".into());
            args.extend([device_id, app_id].map(Into::into));

            let StartAppProtocol {
                log_state,
                experiments,
                reset_db,
                jexl_expression,
            } = app_protocol;

            if log_state || experiments.is_some() || reset_db || jexl_expression.is_some() {
                args.extend(["--", "--nimbus-cli", "--version", "1"].map(Into::into));
            }

            if reset_db {
                // We don't check launch here, because reset-db is never used
                // without enroll.
                args.push("--reset-db".into());
            }

            if let Some(s) = experiments {
                args.push("--experiments".into());
                args.push(s.to_string().replace('\'', "&apos;").into());
            }

            if log_state {
                args.push("--log-state".into());
            }

            if let Some(expr) = jexl_expression {
                let escaped = expr.replace('\'', "&apos;");
                args.push("--eval-jexl".into());
                args.push(escaped.into());
            }

            let sh = format!(r#"xcrun simctl {}"#, args.join(" \\\n        "));
            let mut stdout = StandardStream::stdout(ColorChoice::Auto);
            write_prompt(&mut stdout, &sh)?;

            let mut cmd = self.exe()?;
            cmd.args(args.iter().map(AsRef::as_ref));
            Ok(cmd)
        } else {
            unreachable!()
        }
    }
}

fn logcat_args<'a>() -> Vec<&'a str> {
    vec!["logcat", "-b", "main"]
}

impl NimbusApp {
    fn validate_experiment(
        &self,
        manifest_source: &ManifestSource,
        experiment: &ExperimentSource,
    ) -> Result<bool> {
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        let value: Value = experiment.try_into()?;

        let manifest = match TryInto::<FeatureManifest>::try_into(manifest_source) {
            Ok(manifest) => {
                write_ok(
                    &mut stdout,
                    &format!("Loaded manifest from {manifest_source}"),
                )?;
                manifest
            }
            Err(err) => {
                write_err(
                    &mut stdout,
                    &format!("Problem with manifest from {manifest_source}"),
                    &err.to_string(),
                )?;
                bail!("Error when loading and validating the manifest");
            }
        };

        let mut is_valid = true;
        for b in try_find_branches_from_experiment(&value)? {
            let branch = b.get_str("slug")?;
            for f in try_find_features_from_branch(&b)? {
                let id = f.get_str("featureId")?;
                let value = f
                    .get("value")
                    .unwrap_or_else(|| panic!("Branch {branch} feature {id} has no value"));
                let res = manifest.validate_feature_config(id, value.clone());
                match res {
                    Ok(_) => write_ok(&mut stdout, &format!("{branch: <15} {id}"))?,
                    Err(err) => {
                        is_valid = false;
                        write_err(
                            &mut stdout,
                            &format!("{branch: <15} {id}"),
                            &err.to_string(),
                        )?
                    }
                }
            }
        }
        if !is_valid {
            bail!("At least one error detected");
        }
        Ok(true)
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::io::Write;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use nimbus_fml::intermediate_representation::FeatureManifest;
use serde_json::{Value, json};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::protocol::{LaunchAction, LaunchProtocol};
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

pub fn write_prompt(stream: &mut StandardStream, command: &str) -> Result<()> {
    stream.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
    write!(stream, "$ ")?;
    stream.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
    writeln!(stream, "{}", command)?;
    stream.reset()?;

    Ok(())
}

pub fn write_info(stream: &mut StandardStream, text: &str) -> Result<()> {
    stream.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
    write!(stream, "ℹ️  ")?;
    stream.set_color(ColorSpec::new().set_fg(None))?;
    writeln!(stream, "{}", text)?;
    stream.reset()?;

    Ok(())
}

pub fn write_ok(stream: &mut StandardStream, title: &str) -> Result<()> {
    write!(stream, "✅ ")?;
    stream.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
    writeln!(stream, "{title}")?;
    stream.reset()?;

    Ok(())
}

pub fn write_err(stream: &mut StandardStream, title: &str, detail: &str) -> Result<()> {
    write!(stream, "❎ ")?;
    stream.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
    write!(stream, "{title}")?;
    stream.reset()?;
    writeln!(stream, ": {detail}")?;

    Ok(())
}

impl LaunchableApp {
    fn kill_app(&self) -> Result<bool> {
        match self {
            Self::Android(android_app) => android_app.kill(),
            Self::Ios(ios_app) => ios_app.kill(),
        }
    }

    fn unenroll_all(&self, open: &AppOpenArgs) -> Result<bool> {
        if let Self::Ios(app) = self {
            app.require_simulator("unenroll")?;
        }

        let experiments = Value::try_from(&ExperimentListSource::Empty)?;
        let protocol = LaunchProtocol::setting_experiments(&experiments).logging_state(true);
        self.launch(protocol, open)
    }

    fn reset_app(&self) -> Result<bool> {
        match self {
            Self::Android(android_app) => android_app.reset_app(),
            Self::Ios(ios_app) => ios_app.reset_app(),
        }
    }

    fn tail_logs(&self) -> Result<bool> {
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);

        match self {
            Self::Android(android_app) => android_app.tail_logs(&mut stdout),
            Self::Ios(ios_app) => ios_app.tail_logs(&mut stdout),
        }
    }

    fn capture_logs(&self, file: &Path) -> Result<bool> {
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);

        match self {
            Self::Android(android_app) => android_app.capture_logs(&mut stdout, file),
            Self::Ios(ios_app) => ios_app.capture_logs(&mut stdout, file),
        }
    }

    fn log_state(&self, open: &AppOpenArgs) -> Result<bool> {
        if let Self::Ios(app) = self {
            app.require_simulator("log-state")?;
        }

        let protocol = LaunchProtocol::default().logging_state(true);
        self.launch(protocol, open)
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
        if let Self::Ios(app) = self {
            app.require_simulator("enroll")?;
        }

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

        let experiments = json! {{ "data": recipes }};
        let protocol = LaunchProtocol::setting_experiments(&experiments)
            .resetting_db(!preserve_nimbus_db)
            .logging_state(true);
        self.launch(protocol, open)
    }

    fn apply_list(
        &self,
        open: &AppOpenArgs,
        list: &ExperimentListSource,
        preserve_nimbus_db: &bool,
    ) -> Result<bool> {
        if let Self::Ios(app) = self {
            app.require_simulator("apply-file")?;
        }

        let experiments = Value::try_from(list)?;
        let protocol = LaunchProtocol::setting_experiments(&experiments)
            .resetting_db(!preserve_nimbus_db)
            .logging_state(true);
        self.launch(protocol, open)
    }

    fn open(&self, open: &AppOpenArgs) -> Result<bool> {
        if let Self::Ios(app) = self {
            app.require_simulator("open")?;
        }

        self.launch(Default::default(), open)
    }

    fn eval_jexl(&self, expression: &str, open: &AppOpenArgs) -> Result<bool> {
        match self {
            Self::Android(app) => {
                // Clear logcat before starting to ensure we only read fresh logs
                app.clear_logs()?;
            }

            Self::Ios(app) => {
                app.require_simulator("eval-jexl")?;
            }
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        write_info(
            &mut stdout,
            &format!("Evaluating JEXL expression `{expression}`"),
        )?;

        let protocol = LaunchProtocol::evaluating_jexl(expression);

        if !self.launch(protocol, open)? {
            write_err(&mut stdout, "Error", "Failed to launch the application")?;

            return Err(anyhow!("Could not evaluate JEXL expression"));
        }

        write_info(&mut stdout, "Waiting for result...")?;

        const MAX_RETRIES: u32 = 3;
        const SLEEP_DURATION: Duration = Duration::from_secs(3);

        let mut attempt = 1;
        let rsp = loop {
            sleep(SLEEP_DURATION);

            match self.try_capture_jexl_result()? {
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

    fn try_capture_jexl_result(&self) -> Result<Option<String>> {
        let output = match self {
            Self::Android(android_app) => android_app.try_capture_jexl_result_logs()?,
            Self::Ios(ios_app) => ios_app.try_capture_jexl_result_logs()?,
        };

        let logs = String::from_utf8_lossy(&output);

        for line in logs.lines().rev() {
            if let Some((_, json)) = line.split_once("JEXL_RESULT: ") {
                return Ok(Some(json.into()));
            }
        }

        Ok(None)
    }

    fn launch(&self, app_protocol: LaunchProtocol, open: &AppOpenArgs) -> Result<bool> {
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        if let Some(file) = &open.output {
            if let Some(LaunchAction::SetExperiments(contents)) = app_protocol.action {
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

        let mut cmd = match self {
            Self::Android(android_app) => android_app.build_launch_cmd(app_protocol)?,
            Self::Ios(ios_app) => ios_app.build_launch_cmd(app_protocol)?,
        };

        Ok(cmd.spawn()?.wait()?.success())
    }
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

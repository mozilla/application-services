// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use termcolor::{ColorChoice, StandardStream};

use crate::AppOpenArgs;
use crate::cmd::{write_info, write_prompt};

/// Instructions for an Nimbus client application.
///
/// This determines if the nimbus-cli integration in the application will be
/// invoked and what actions it will take.
#[derive(Clone, Debug, Default)]
pub struct LaunchProtocol<'a> {
    /// Reset the database before invoking the given action.
    pub reset_db: bool,

    /// Log the state of the internal enrollment store after performing the
    /// action.
    pub log_state: bool,

    /// An optional action to perform.
    pub action: Option<LaunchAction<'a>>,
}

/// An action the application will take upon opening.
#[derive(Clone, Debug)]
pub enum LaunchAction<'a> {
    /// Set the set of local experiments to the given payload and then apply
    /// them.
    SetExperiments(&'a Value),

    /// Evaluate the given JEXL expression and print the result.
    EvalJexl(&'a str),
}

impl<'a> LaunchProtocol<'a> {
    pub fn setting_experiments(experiments: &'a Value) -> Self {
        Self {
            action: Some(LaunchAction::SetExperiments(experiments)),
            ..Default::default()
        }
    }

    pub fn evaluating_jexl(jexl_expression: &'a str) -> Self {
        Self {
            action: Some(LaunchAction::EvalJexl(jexl_expression)),
            ..Default::default()
        }
    }

    pub fn logging_state(self, log_state: bool) -> Self {
        Self { log_state, ..self }
    }

    pub fn resetting_db(self, reset_db: bool) -> Self {
        Self { reset_db, ..self }
    }

    pub fn has_actions(&self) -> bool {
        self.reset_db || self.log_state || self.action.is_some()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchableApp {
    Android(AndroidApp),

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Ios(IosApp),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndroidApp {
    pub device_id: Option<String>,
    pub package_name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
enum DeviceKind {
    Simulator,
    Phsyical,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IosApp {
    pub device_id: String,
    pub app_id: String,

    device_kind: RefCell<Option<DeviceKind>>,
}

impl AndroidApp {
    pub fn build_launch_cmd(&self, protocol: LaunchProtocol, open: &AppOpenArgs) -> Result<Command> {
        let mut args: Vec<String> = Vec::new();

        let (start_args, ending_args) = open.args();
        args.extend_from_slice(start_args);

        args.extend([
            format!("-p {}", self.package_name),
            "-a org.mozilla.fenix.NIMBUS_TOOLING".to_string(),
            "-f 0x20".to_string(), // android.content.Intent.FLAG_INCLUDE_STOPPED_PACKAGES
        ]);

        if protocol.has_actions() {
            args.extend(["--esn nimbus-cli".to_string(), "--ei version 1".to_string()]);

            if protocol.reset_db {
                args.push("--ez reset-db true".to_string());
            }

            if protocol.log_state {
                args.push("--ez log-state true".to_string());
            }

            match protocol.action {
                Some(LaunchAction::SetExperiments(experiments)) => {
                    let json = experiments.to_string().replace('\'', "&apos;");
                    args.push(format!("--es experiments '{}'", json))
                }

                Some(LaunchAction::EvalJexl(expr)) => {
                    let escaped = expr.replace('\'', "&apos;");
                    args.push(format!("--es eval-jexl '{}'", escaped));
                }

                None => {}
            }
        }

        args.extend_from_slice(ending_args);

        let sh = format!(r#"am broadcast {}"#, args.join(" \\\n        "));
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        write_prompt(&mut stdout, &format!("adb shell \"{}\"", sh))?;

        let mut cmd = self.adb();
        cmd.arg("shell").arg(&sh);
        Ok(cmd)
    }

    pub fn kill(&self) -> Result<bool> {
        Ok(self
            .adb()
            .arg("shell")
            .arg(format!("am force-stop {}", &self.package_name))
            .spawn()?
            .wait()?
            .success())
    }

    pub fn reset_app(&self) -> Result<bool> {
        Ok(self
            .adb()
            .arg("shell")
            .arg(format!("pm clear {}", &self.package_name))
            .spawn()?
            .wait()?
            .success())
    }

    pub fn tail_logs(&self, stream: &mut StandardStream) -> Result<bool> {
        let mut args = self.get_logcat_args();
        args.append(&mut vec!["-v", "color"]);
        write_prompt(stream, &format!("adb {}", args.join(" ")))?;
        Ok(self.adb().args(args).spawn()?.wait()?.success())
    }

    pub fn capture_logs(&self, stream: &mut StandardStream, dest: &Path) -> Result<bool> {
        let mut args = self.get_logcat_args();
        args.append(&mut vec!["-d"]);
        write_prompt(
            stream,
            &format!("adb {} > {}", args.join(" "), dest.display()),
        )?;
        let output = self.adb().args(args).output()?;
        std::fs::write(dest, String::from_utf8_lossy(&output.stdout).to_string())?;
        Ok(true)
    }

    pub fn try_capture_jexl_result_logs(&self) -> Result<Vec<u8>> {
        Ok(self
            .adb()
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
            .stdout)
    }

    pub fn clear_logs(&self) -> Result<()> {
        let output = self.adb().args(["logcat", "-c"]).output()?;

        if !output.status.success() {
            bail!("{}", String::from_utf8_lossy(&output.stderr));
        }

        Ok(())
    }

    fn get_logcat_args(&self) -> Vec<&'static str> {
        vec!["logcat", "-b", "main"]
    }

    fn adb(&self) -> Command {
        const ADB: &'static str = cfg_select! {
            target_os = "windows" => "adb.exe",
            _ => "adb",
        };

        let adb = std::env::var("ADB_PATH").unwrap_or_else(|_| ADB.into());

        let mut cmd = Command::new(adb);
        if let Some(id) = &self.device_id {
            cmd.args(["-s", id]);
        }

        cmd
    }
}

impl IosApp {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn new(device_id: String, app_id: String) -> Self {
        Self {
            device_id,
            app_id,
            device_kind: RefCell::new(None),
        }
    }

    pub fn build_launch_cmd(&self, protocol: LaunchProtocol, open: &AppOpenArgs) -> Result<Command> {
        let mut args: Vec<String> = Vec::new();
        let (starting_args, ending_args) = open.args();

        args.push("launch".to_string());
        args.extend_from_slice(starting_args);
        args.extend([self.device_id.to_string(), self.app_id.to_string()]);

        if protocol.has_actions() {
            args.extend([
                "--nimbus-cli".to_string(),
                "--version".to_string(),
                "1".to_string(),
            ]);

            if protocol.reset_db {
                args.push("--reset-db".to_string());
            }

            if protocol.log_state {
                args.push("--log-state".to_string());
            }

            match protocol.action {
                Some(LaunchAction::SetExperiments(experiments)) => {
                    args.extend([
                        "--experiments".to_string(),
                        experiments.to_string().replace('\'', "&apos;"),
                    ]);
                }

                Some(LaunchAction::EvalJexl(jexl_expression)) => {
                    args.extend([
                        "--eval-jexl".to_string(),
                        jexl_expression.replace('\'', "&apos;"),
                    ]);
                }

                None => {}
            }
        }

        args.extend_from_slice(ending_args);

        let sh = format!(r#"xcrun simctl {}"#, args.join(" \\\n        "),);
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        write_prompt(&mut stdout, &sh)?;

        let mut cmd = Self::xcrun_simctl();
        cmd.args(args.clone());
        Ok(cmd)
    }

    pub fn kill(&self) -> Result<bool> {
        let _ = Self::xcrun_simctl()
            .args(["terminate", &self.device_id, &self.app_id])
            .output()?;
        Ok(true)
    }

    pub fn reset_app(&self) -> Result<bool> {
        Self::xcrun_simctl()
            .args(["privacy", &self.device_id, "reset", "all", &self.app_id])
            .status()?;
        let data_dir = self.get_app_container("data")?;
        let groups = self.get_app_container("groups")?;

        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        write_info(&mut stdout, "Resetting the app")?;

        if !data_dir.is_empty() {
            write_prompt(&mut stdout, &format!("rm -Rf {}/* 2>/dev/null", data_dir))?;
            let _ = std::fs::remove_dir_all(&data_dir);
            let _ = std::fs::create_dir_all(&data_dir);
        }
        let lines = groups.split('\n');

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

    pub fn tail_logs(&self, stream: &mut StandardStream) -> Result<bool> {
        write_prompt(
            stream,
            &format!("{} | xargs tail -f", self.get_log_file_command()),
        )?;
        let log_path = self.get_log_path()?;

        Ok(Command::new("tail")
            .arg("-f")
            .arg(log_path.as_path().to_str().unwrap())
            .spawn()?
            .wait()?
            .success())
    }

    pub fn capture_logs(&self, stream: &mut StandardStream, dest: &Path) -> Result<bool> {
        let log = self.get_log_path()?;
        write_prompt(
            stream,
            &format!(
                "{} | xargs -J %log_file% cp %log_file% {}",
                self.get_log_file_command(),
                dest.display(),
            ),
        )?;
        std::fs::copy(log, dest)?;
        Ok(true)
    }

    pub fn try_capture_jexl_result_logs(&self) -> Result<Vec<u8>> {
        Ok(Self::xcrun_simctl()
            .args([
                "spawn",
                &self.device_id,
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
            .stdout)
    }

    fn get_log_file_command(&self) -> String {
        format!(
            "find $(xcrun simctl get_app_container {0} {1} data) -name \\*.log",
            &self.device_id, &self.app_id
        )
    }

    fn get_app_container(&self, container: &str) -> Result<String> {
        // We need to get the app container directories, and delete them.
        let output = Self::xcrun_simctl()
            .args([
                "get_app_container",
                &self.device_id,
                &self.app_id,
                container,
            ])
            .output()
            .expect("Expected an app-container from the simulator");

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn get_log_path(&self) -> Result<PathBuf> {
        let data = self.get_app_container("data")?;
        let mut files = glob::glob(&format!("{}/**/*.log", data))?;
        let log = files.next();
        Ok(log.ok_or_else(|| {
            anyhow::Error::msg(
                "Logs are not available before the app is started for the first time",
            )
        })??)
    }

    fn get_device_kind(&self) -> Result<DeviceKind> {
        if let Some(device_kind) = *self.device_kind.borrow() {
            return Ok(device_kind);
        }

        let device_kind = Self::query_device_kind(&self.device_id)?;
        self.device_kind.replace(Some(device_kind));

        Ok(device_kind)
    }

    fn query_device_kind(device_id: &str) -> Result<DeviceKind> {
        if device_id == "booted" {
            // "booted" is a special device we can pass to simctl to interact
            // with the currently running simulator. However, `devicectl info`
            // won't resolve "booted" to a simulator and will instead return an
            // error.
            return Ok(DeviceKind::Simulator);
        }

        let output = Self::xcrun_devicectl()
            .args([
                "device",
                "info",
                "details",
                "-q",
                "-j",
                "-",
                "--device",
                device_id,
            ])
            .stderr(Stdio::null())
            .output()?;

        const PARSE_ERROR: &'static str = "could not parse devicectl output";
        const BOOT_STATE_BOOTED: &'static str = "booted";
        const REALITY_PHYSICAL: &'static str = "physical";
        const REALITY_SIMULATOR: &'static str = "simulator";

        if output.status.success() {
            let json =
                serde_json::from_slice::<serde_json::Value>(&output.stdout).context(PARSE_ERROR)?;
            let result = json
                .as_object()
                .and_then(|obj| obj.get("result")?.as_object());

            let boot_state = result
                .and_then(|obj| {
                    obj.get("deviceProperties")?
                        .as_object()?
                        .get("bootState")?
                        .as_str()
                })
                .context(PARSE_ERROR)?;

            if boot_state != BOOT_STATE_BOOTED {
                return Err(anyhow!("Device {} is not booted", device_id));
            }

            let reality = result
                .and_then(|obj| {
                    obj.get("hardwareProperties")?
                        .as_object()?
                        .get("reality")?
                        .as_str()
                })
                .context(PARSE_ERROR)?;

            match reality {
                REALITY_PHYSICAL => Ok(DeviceKind::Phsyical),
                REALITY_SIMULATOR => Ok(DeviceKind::Simulator),
                _ => Err(anyhow!("Unknown device reality {reality}")),
            }
        } else {
            let json =
                serde_json::from_slice::<serde_json::Value>(&output.stdout).context(PARSE_ERROR)?;
            let err = json
                .as_object()
                .and_then(|rsp| {
                    rsp.get("error")?
                        .as_object()?
                        .get("userInfo")?
                        .as_object()?
                        .get("NSLocalizedDescription")?
                        .as_object()?
                        .get("string")?
                        .as_str()
                })
                .context(PARSE_ERROR)?;

            Err(anyhow!("devicectl error: {}", err))
        }
    }

    fn xcrun_simctl() -> Command {
        let mut cmd = Self::xcrun();
        cmd.arg("simctl");
        cmd
    }

    fn xcrun_devicectl() -> Command {
        let mut cmd = Self::xcrun();
        cmd.arg("devicectl");
        cmd
    }

    fn xcrun() -> Command {
        let xcrun = std::env::var("XCRUN_PATH").unwrap_or_else(|_| "xcrun".to_string());
        Command::new(xcrun)
    }
}

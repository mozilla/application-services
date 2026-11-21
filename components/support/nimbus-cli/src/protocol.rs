// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::borrow::Cow;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use serde_json::Value;
use termcolor::{ColorChoice, StandardStream};

use crate::cmd::{write_err, write_info, write_prompt};

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
pub enum DeviceKind {
    Simulator,
    Physical,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IosApp {
    pub device_id: String,
    pub app_id: String,

    device_kind: RefCell<Option<DeviceKind>>,
}

impl AndroidApp {
    pub fn build_launch_cmd(&self, protocol: LaunchProtocol) -> Result<Command> {
        let mut args = Vec::<Cow<str>>::new();

        if protocol.has_actions() {
            args.extend(["am", "broadcast"].map(Into::into));
            args.extend([
                "-p".into(),
                Cow::Borrowed(&*self.package_name),
                "-a".into(),
                "org.mozilla.fenix.NIMBUS_TOOLING".into(),
                "-f".into(),
                // android.content.Intent.FLAG_INCLUDE_STOPPED_PACKAGES
                "0x20".into(),
            ]);
            args.extend(["--esn", "nimbus-cli", "--ei", "version", "1"].map(Into::into));

            if protocol.reset_db {
                args.extend(["--ez", "reset-db", "true"].map(Into::into));
            }

            if protocol.log_state {
                args.extend(["--ez", "log-state", "true"].map(Into::into));
            }

            match protocol.action {
                Some(LaunchAction::SetExperiments(experiments)) => {
                    let json = experiments.to_string().replace('\'', "&apos;");
                    args.extend(["--es".into(), "experiments".into(), json.into()]);
                }

                Some(LaunchAction::EvalJexl(expr)) => {
                    let escaped = expr.replace('\'', "&apos;");
                    args.extend(["--es", "eval-jexl"].map(Into::into));
                    args.push(escaped.into());
                }

                None => {}
            }
        } else {
            args.extend(["am", "start"].map(Into::into));
            args.push("-n".into());
            args.push(format!("{}/.App", self.package_name).into());
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

        let mut cmd = self.adb();
        cmd.args(["shell", "--"])
            .args(args.iter().map(AsRef::as_ref));

        Ok(cmd)
    }

    pub fn kill(&self) -> Result<bool> {
        Ok(self
            .adb()
            .args(["shell", "--", "am", "force-stop", &self.package_name])
            .spawn()?
            .wait()?
            .success())
    }

    pub fn reset_app(&self) -> Result<bool> {
        Ok(self
            .adb()
            .args(["shell", "--", "pm", "clear", &self.package_name])
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
        #[cfg(target_os = "windows")]
        const ADB: &'static str = "adb.exe";

        #[cfg(not(target_os = "windows"))]
        const ADB: &'static str = "adb";

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

    pub fn build_launch_cmd(&self, protocol: LaunchProtocol) -> Result<Command> {
        let mut args: Vec<Cow<str>> = ["simctl", "launch", &self.device_id, &self.app_id]
            .map(Into::into)
            .into();

        if protocol.has_actions() {
            args.extend(["--", "--nimbus-cli", "--version", "1"].map(Into::into));

            if protocol.reset_db {
                args.push("--reset-db".into());
            }

            if protocol.log_state {
                args.push("--log-state".into());
            }

            match protocol.action {
                Some(LaunchAction::SetExperiments(experiments)) => {
                    args.push("--experiments".into());
                    args.push(experiments.to_string().replace('\'', "&apos;").into());
                }

                Some(LaunchAction::EvalJexl(jexl_expression)) => {
                    let escaped = jexl_expression.replace('\'', "&apos;");
                    args.push("--eval-jexl".into());
                    args.push(escaped.into());
                }

                None => {}
            }
        }

        let sh = format!(r#"xcrun {}"#, args.join(" \\\n        "));
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        write_prompt(&mut stdout, &sh)?;

        let mut cmd = Self::xcrun();
        cmd.args(args.iter().map(AsRef::as_ref));
        Ok(cmd)
    }

    pub fn kill(&self) -> Result<bool> {
        self.require_simulator("open")?;

        let _ = Self::xcrun()
            .args(["simctl", "terminate", &self.device_id, &self.app_id])
            .output()?;
        Ok(true)
    }

    pub fn reset_app(&self) -> Result<bool> {
        self.require_simulator("reset-app")?;

        Self::xcrun()
            .args([
                "simctl",
                "privacy",
                &self.device_id,
                "reset",
                "all",
                &self.app_id,
            ])
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
        self.require_simulator("tail-logs")?;

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
        self.require_simulator("capture-logs")?;

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
        assert_eq!(self.get_device_kind()?, DeviceKind::Simulator);

        Ok(Self::xcrun()
            .args([
                "simctl",
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
        assert_eq!(self.get_device_kind()?, DeviceKind::Simulator);

        // We need to get the app container directories, and delete them.
        let output = Self::xcrun()
            .args([
                "simctl",
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

        files
            .next()
            .transpose()?
            .context("Logs are not available before the app is started for the first time")
    }

    pub fn require_simulator(&self, command: &str) -> Result<()> {
        match self.get_device_kind()? {
            DeviceKind::Physical => {
                let mut stdout = StandardStream::stdout(ColorChoice::Auto);
                write_err(&mut stdout, command, "unsupported on physical devices")?;

                Err(anyhow!("Unsupported on physical devices"))
            }

            DeviceKind::Simulator => Ok(()),
        }
    }

    pub fn get_device_kind(&self) -> Result<DeviceKind> {
        if let Some(device_kind) = *self.device_kind.borrow() {
            return Ok(device_kind);
        }

        let device_kind = Self::query_device_kind(&self.device_id)?;
        self.device_kind.replace(Some(device_kind));

        Ok(device_kind)
    }

    fn query_device_kind(device_id: &str) -> Result<DeviceKind> {
        const BOOT_STATE_BOOTED: &'static str = "booted";
        const REALITY_PHYSICAL: &'static str = "physical";
        const REALITY_SIMULATOR: &'static str = "simulator";

        if device_id == "booted" {
            // "booted" is a special device we can pass to simctl to interact
            // with the currently running simulator. However, `devicectl info`
            // won't resolve "booted" to a simulator and will instead return an
            // error.
            return Ok(DeviceKind::Simulator);
        }

        Self::require_devicectl()?;

        let output = Self::xcrun()
            .args([
                "devicectl",
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

        if !output.status.success() {
            Self::report_devicectl_error(
                "devicectl",
                &format!("Could not query information about device {device_id}"),
                &output.stdout,
            )?;
        }

        let details =
            serde_json::from_slice::<DeviceCtlRsp<DeviceCtlDeviceDetails>>(&output.stdout)
                .with_context(|| {
                    format!(
                        "Could not get information about device {} from devicectl",
                        device_id
                    )
                })?
                .result;

        if details.device_properties.boot_state != BOOT_STATE_BOOTED {
            bail!("Device {} is not booted", device_id);
        }

        match &*details.hardware_properties.reality {
            REALITY_PHYSICAL => Ok(DeviceKind::Physical),
            REALITY_SIMULATOR => Ok(DeviceKind::Simulator),
            _ => Err(anyhow!(
                "Unknown device reality {}",
                details.hardware_properties.reality
            )),
        }
    }

    fn require_devicectl() -> Result<()> {
        const PARSE_ERROR: &'static str = "Could not parse devicectl version";
        const IS_IT_INSTALLED: &'static str = "Could not query devicectl version: is it installed?";

        let output = Self::xcrun()
            .args(["devicectl", "--version"])
            .output()
            .context(IS_IT_INSTALLED)?;

        if !output.status.success() {
            bail!(IS_IT_INSTALLED);
        }

        let output = String::from_utf8_lossy(&output.stdout);
        let Some((major, minor)) = output.trim().split_once('.') else {
            bail!(PARSE_ERROR);
        };
        let (Ok(major), Ok(minor)) = (u32::from_str(major), u32::from_str(minor)) else {
            bail!(PARSE_ERROR);
        };

        if (major, minor) < (651, 31) {
            bail!(
                "Your version of devicectl ({major}.{minor}) is too old; at least 651.31 is required"
            );
        }

        Ok(())
    }

    fn report_devicectl_error(title: &str, detail: &str, rsp: &[u8]) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        write_err(&mut stdout, title, detail)?;

        let json = serde_json::from_slice::<serde_json::Value>(rsp).with_context(|| {
            format!(
                "Failed to parse devicectl error: {}",
                String::from_utf8_lossy(rsp)
            )
        })?;
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
            .with_context(|| {
                format!(
                    "Failed to parse devicectl error: {}",
                    String::from_utf8_lossy(rsp)
                )
            })?;

        Err(anyhow!("devicectl error: {}", err))
    }

    fn xcrun() -> Command {
        let xcrun = std::env::var("XCRUN_PATH").unwrap_or_else(|_| "xcrun".to_string());
        Command::new(xcrun)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCtlRsp<T> {
    result: T,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCtlDeviceDetails {
    device_properties: DeviceCtlDeviceProperties,
    hardware_properties: DeviceCtlHardwareProperties,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCtlDeviceProperties {
    boot_state: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCtlHardwareProperties {
    reality: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCtlApplicationList {
    apps: Vec<DeviceCtlApplicationInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCtlApplicationInfo {
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCtlProcessList {
    running_processes: Vec<DeviceCtlProcessInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCtlProcessInfo {
    executable: String,
    process_identifier: u32,
}

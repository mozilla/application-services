// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::{Path, PathBuf};
use std::process::{Command};

use anyhow::{Result, bail};
use serde_json::Value;
use termcolor::{StandardStream, ColorChoice};

use crate::cmd::{write_info, write_prompt};
use crate::AppOpenArgs;

/// This is the protocol that each app understands.
///
/// It is sent to the apps via the start_app command.
///
/// Any change to this protocol requires changing the Kotlin and Swift code,
/// and perhaps the app code itself.
#[derive(Clone, Debug, Default)]
pub(crate) struct StartAppProtocol<'a> {
    pub(crate) reset_db: bool,
    pub(crate) experiments: Option<&'a Value>,
    pub(crate) log_state: bool,
    pub(crate) jexl_expression: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchableApp {
    Android(AndroidApp),

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Ios(IosApp)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AndroidApp {
    pub device_id: Option<String>,
    pub package_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IosApp {
    pub device_id: String,
    pub app_id: String,
}

impl AndroidApp {
    pub fn build_launch_cmd(&self, protocol: StartAppProtocol, open: &AppOpenArgs) -> Result<Command> {
        let mut args: Vec<String> = Vec::new();

        let (start_args, ending_args) = open.args();
        args.extend_from_slice(start_args);

        args.extend([
            format!("-p {}", self.package_name),
            "-a org.mozilla.fenix.NIMBUS_TOOLING".to_string(),
            "-f 0x20".to_string(), // android.content.Intent.FLAG_INCLUDE_STOPPED_PACKAGES
        ]);

        let StartAppProtocol {
            reset_db,
            experiments,
            log_state,
            jexl_expression,
        } = protocol;

        if log_state || experiments.is_some() || reset_db || jexl_expression.is_some() {
            args.extend(["--esn nimbus-cli".to_string(), "--ei version 1".to_string()]);
        }

        if reset_db {
            args.push("--ez reset-db true".to_string());
        }

        if let Some(s) = experiments {
            let json = s.to_string().replace('\'', "&apos;");
            args.push(format!("--es experiments '{}'", json))
        }

        if log_state {
            args.push("--ez log-state true".to_string());
        }

        if let Some(expr) = jexl_expression {
            let escaped = expr.replace('\'', "&apos;");
            args.push(format!("--es eval-jexl '{}'", escaped));
        };

        args.extend_from_slice(ending_args);

        let sh = format!(r#"am broadcast {}"#, args.join(" \\\n        "));
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        write_prompt(&mut stdout, &format!("adb shell \"{}\"", sh))?;

        let mut cmd = self.adb();
        cmd.arg("shell").arg(&sh);
        Ok(cmd)
    }

    pub fn kill(&self) -> Result<bool> {
        Ok(
            self.adb()
                .arg("shell")
                .arg(format!("am force-stop {}", &self.package_name))
                .spawn()?
                .wait()?
                .success()
        )
    }

    pub fn reset_app(&self) -> Result<bool> {
        Ok(
            self.adb()
                .arg("shell")
                .arg(format!("pm clear {}", &self.package_name))
                .spawn()?
                .wait()?
                .success()
        )
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
            &format!(
                "adb {} > {}",
                args.join(" "),
                dest.display()
            ),
        )?;
        let output = self.adb().args(args).output()?;
        std::fs::write(dest, String::from_utf8_lossy(&output.stdout).to_string())?;
        Ok(true)
    }

    pub fn try_capture_jexl_result_logs(&self) -> Result<Vec<u8>> {
        Ok(
            self.adb()
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
        )
    }

    pub fn clear_logs(&self) -> Result<()> {
        let output = self.adb()
                .args(["logcat", "-c"])
                .output()?;

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
    pub fn build_launch_cmd(&self, protocol: StartAppProtocol, open: &AppOpenArgs) -> Result<Command> {
        let mut args: Vec<String> = Vec::new();

        let (starting_args, ending_args) = open.args();

        args.push("launch".to_string());
        args.extend_from_slice(starting_args);
        args.extend([self.device_id.to_string(), self.app_id.to_string()]);

        let StartAppProtocol {
            log_state,
            experiments,
            reset_db,
            jexl_expression,
        } = protocol;

        if log_state || experiments.is_some() || reset_db || jexl_expression.is_some() {
            args.extend([
                "--nimbus-cli".to_string(),
                "--version".to_string(),
                "1".to_string(),
            ]);
        }

        if reset_db {
            // We don't check launch here, because reset-db is never used
            // without enroll.
            args.push("--reset-db".to_string());
        }

        if let Some(s) = experiments {
            args.extend([
                "--experiments".to_string(),
                s.to_string().replace('\'', "&apos;"),
            ]);
        }

        if log_state {
            args.push("--log-state".to_string());
        }

        if let Some(expr) = jexl_expression {
            args.extend(["--eval-jexl".to_string(), expr.replace('\'', "&apos;")]);
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

        Ok(
            Command::new("tail")
            .arg("-f")
            .arg(log_path.as_path().to_str().unwrap())
            .spawn()?
            .wait()?
            .success()
        )
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
        Ok(
            Self::xcrun_simctl()
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
                .stdout
        )
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
            .args(["get_app_container", &self.device_id, &self.app_id, container])
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

    fn xcrun_simctl() -> Command {
        let xcrun = std::env::var("XCRUN_PATH").unwrap_or_else(|_| "xcrun".to_string());
        let mut cmd = Command::new(xcrun);
        cmd.arg("simctl");
        cmd
    }
}

// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use termcolor::{ColorChoice, StandardStream};

use crate::cmd::{write_info, write_prompt};

/// This is the protocol that each app understands.
///
/// It is sent to the apps via the `launch()` API.
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
    Ios(IosApp),
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
    pub fn build_launch_cmd(&self, protocol: StartAppProtocol) -> Result<Command> {
        let mut args = Vec::<Cow<str>>::new();

        let StartAppProtocol {
            reset_db,
            experiments,
            log_state,
            jexl_expression,
        } = protocol;

        if log_state || experiments.is_some() || reset_db || jexl_expression.is_some() {
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
    pub fn build_launch_cmd(&self, protocol: StartAppProtocol) -> Result<Command> {
        let mut args: Vec<Cow<str>> = ["simctl", "launch", &self.device_id, &self.app_id]
            .map(Into::into)
            .into();

        let StartAppProtocol {
            reset_db,
            experiments,
            log_state,
            jexl_expression,
        } = protocol;

        if log_state || experiments.is_some() || reset_db || jexl_expression.is_some() {
            args.extend(["--", "--nimbus-cli", "--version", "1"].map(Into::into));

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
        }

        let sh = format!(r#"xcrun {}"#, args.join(" \\\n        "));
        let mut stdout = StandardStream::stdout(ColorChoice::Auto);
        write_prompt(&mut stdout, &sh)?;

        let mut cmd = Self::xcrun();
        cmd.args(args.iter().map(AsRef::as_ref));
        Ok(cmd)
    }

    pub fn kill(&self) -> Result<bool> {
        let _ = Self::xcrun()
            .args(["simctl", "terminate", &self.device_id, &self.app_id])
            .output()?;
        Ok(true)
    }

    pub fn reset_app(&self) -> Result<bool> {
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

    fn xcrun() -> Command {
        let xcrun = std::env::var("XCRUN_PATH").unwrap_or_else(|_| "xcrun".to_string());
        Command::new(xcrun)
    }
}

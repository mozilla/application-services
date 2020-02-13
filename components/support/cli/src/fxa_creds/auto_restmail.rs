use super::*;
use fxa_client::{Config, FirefoxAccount};
use std::path::Path;
lazy_static::lazy_static! {
    // Figures out where `sync-test/helper` lives. This is pretty gross, but once
    // https://github.com/rust-lang/cargo/issues/2841 is resolved it should be simpler.
    // That said, it's possible we should probably just rewrite that script in rust instead :p.
    static ref HELPER_SCRIPT_DIR: std::path::PathBuf = {
        let mut path = std::env::current_exe().expect("Failed to get current exe path...");
        // Find `target` which should contain this program.
        while path.file_name().expect("Failed to find target!") != "target" {
            path.pop();
        }
        // And go up once more, to the root of the workspace.
        path.pop();
        // TODO: it would be nice not to hardcode these given that we're
        // planning on moving stuff around, but such is life.
        path.push("testing");
        path.push("sync-test");
        path.push("helper");
        path
    };
}

fn run_helper_command(cmd: &str, cmd_args: &[&str]) -> Result<String> {
    use std::process::{self, Command};
    // This `Once` is used to run `npm install` first time through.
    static HELPER_SETUP: std::sync::Once = std::sync::Once::new();
    HELPER_SETUP.call_once(|| {
        let dir = &*HELPER_SCRIPT_DIR;

        // Let users know why this is happening even if `log` isn't enabled.
        log::info!("Running `npm install` in `integration-test-helper` to ensure it's usable");

        let mut child = Command::new("npm")
            .args(&["install"])
            .current_dir(dir)
            .spawn()
            .expect("Failed to run helper");

        child.wait().expect("Failed to run helper");
    });
    // We should still be in the script dir from HELPER_SETUP's call_once.
    log::info!("Running helper script with command \"{}\"", cmd);

    // node_args = ["index.js", cmd, ...cmd_args] in JavaScript parlance.
    let node_args: Vec<&str> = ["index.js", cmd]
        .iter()
        .chain(cmd_args.iter())
        .cloned() // &&str -> &str
        .collect();

    let child = Command::new("node")
        .args(&node_args)
        .current_dir(&*HELPER_SCRIPT_DIR)
        // Grab stdout, but inherit stderr.
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::inherit())
        .spawn()?;

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let exit_reason = output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "(process terminated by signal)".to_string());
        // Print stdout in case something helpful was logged there, as well as the exit status
        log::info!(
            "Helper script exited with {}, it's stdout was:```\n{}\n```",
            exit_reason,
            String::from_utf8_lossy(&output.stdout)
        );
        failure::bail!("Failed to run helper script");
    }
    // Note: from_utf8_lossy returns a Cow
    let result = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(result)
}

pub fn load_or_create(
    email: &str,
    pass: &str,
    credentials_json: &Path,
    cfg: &Config,
) -> Result<CliFxa> {
    if !email.ends_with("@restmail.net") {
        failure::bail!("Automatic restmail account should have `@restmail.net` on it.");
    }
    if let Ok(acct) = std::fs::read_to_string(credentials_json)
        .map_err(failure::Error::from)
        .and_then(|s| Ok(FirefoxAccount::from_json(&s)?))
    {
        return CliFxa::new(acct, cfg, tokenserver_env());
    }

    log::info!("Creating fx account");

    // `create` doesn't return anything we care about.
    let auth_url = cfg.auth_url()?;
    run_helper_command("create", &[&email, &pass, auth_url.as_str()])?;
    log::info!("Doing oauth flow!");

    let mut fxa = FirefoxAccount::with_config(cfg.clone());
    let oauth_uri = fxa.begin_oauth_flow(&[SYNC_SCOPE])?;
    let auth_url = cfg.auth_url()?;
    let redirected_to =
        run_helper_command("oauth", &[&email, &pass, auth_url.as_str(), &oauth_uri])?;

    log::info!("Helper command gave '{}'", redirected_to);
    complete_oauth(&mut fxa, &credentials_json, &Url::parse(&redirected_to)?)?;

    CliFxa::new(fxa, cfg, tokenserver_env())
}

fn tokenserver_env() -> Option<url::Url> {
    option_env!("TOKENSERVER_URL").map(|env_var| {
        // We hard error here even though we want to return a Result to provide a clearer
        // error for misconfiguration
        url::Url::parse(env_var).expect("Failed to parse TOKENSERVER_URL environment variable!")
    })
}

pub struct AutoDeleted {
    pub email: String,
    pub password: String,
    pub config: Config,
    pub cli: CliFxa,
}

impl Drop for AutoDeleted {
    fn drop(&mut self) {
        let auth_url = self.config.auth_url().unwrap(); // We already parsed this once.
        if let Err(e) =
            run_helper_command("destroy", &[&self.email, &self.password, auth_url.as_str()])
        {
            log::warn!("Failed to destroy fxacct {}: {}", self.email, e);
        }
    }
}
impl AutoDeleted {
    pub fn new(email: String, pass: String, credentials_json: &Path, cfg: Config) -> Result<Self> {
        let cli = load_or_create(&email, &pass, credentials_json, &cfg)?;
        Ok(Self {
            email,
            password: pass,
            config: cfg,
            cli,
        })
    }
}

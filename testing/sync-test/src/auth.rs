/* Any copyright is dedicated to the Public Domain.
http://creativecommons.org/publicdomain/zero/1.0/ */

use fxa_client::{self, Config as FxaConfig, FirefoxAccount};
use logins::PasswordEngine;
use std::collections::HashMap;
use std::sync::{Arc, Once, ONCE_INIT};
use sync15::{KeyBundle, Sync15StorageClientInit};
use url::Url;

pub const CLIENT_ID: &str = "98adfa37698f255b"; // Hrm...
pub const SYNC_SCOPE: &str = "https://identity.mozilla.com/apps/oldsync";

// TODO: This is wrong for dev?
pub const REDIRECT_URI: &str = "https://lockbox.firefox.com/fxa/ios-redirect.html";

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

fn run_helper_command(cmd: &str, cmd_args: &[&str]) -> Result<String, failure::Error> {
    use std::process::{self, Command};
    // This `Once` is used to run `npm install` first time through.
    static HELPER_SETUP: Once = ONCE_INIT;
    HELPER_SETUP.call_once(|| {
        let dir = &*HELPER_SCRIPT_DIR;
        std::env::set_current_dir(dir).expect("Failed to change directory...");

        // Let users know why this is happening even if `log` isn't enabled.
        println!("Running `npm install` in `integration-test-helper` to ensure it's usable");

        let mut child = Command::new("npm")
            .args(&["install"])
            .spawn()
            .expect("Failed to spawn `npm install`! (This test currently requires `node`)");

        child
            .wait()
            .expect("Failed to install helper dependencies, can't run integration test");
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
        println!(
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

// It's important that this doesn't implement Clone! (It destroys it's temporary fxaccount on drop)
#[derive(Debug)]
pub struct TestAccount {
    pub email: String,
    pub pass: String,
    pub cfg: FxaConfig,
}

impl TestAccount {
    fn new(
        email: String,
        pass: String,
        cfg: FxaConfig,
    ) -> Result<Arc<TestAccount>, failure::Error> {
        log::info!("Creating temporary fx account");
        // `create` doesn't return anything we care about.
        let auth_url = cfg.auth_url()?;
        run_helper_command("create", &[&email, &pass, auth_url.as_str()])?;
        Ok(Arc::new(TestAccount { email, pass, cfg }))
    }

    pub fn new_random() -> Result<Arc<TestAccount>, failure::Error> {
        use rand::{self, prelude::*};
        let mut rng = thread_rng();
        let name = format!(
            "rust-login-sql-test--{}",
            rng.sample_iter(&rand::distributions::Alphanumeric)
                .take(5)
                .collect::<String>()
        );
        // Just use the username for the password in case we need to clean these
        // up easily later because of some issue.
        let password = name.clone();
        let email = format!("{}@restmail.net", name);
        Self::new(
            email,
            password,
            FxaConfig::stable_dev(CLIENT_ID, REDIRECT_URI),
        )
    }
}

impl Drop for TestAccount {
    fn drop(&mut self) {
        log::info!("Cleaning up temporary firefox account");
        let auth_url = self.cfg.auth_url().unwrap(); // We already parsed this once.
        if let Err(e) = run_helper_command("destroy", &[&self.email, &self.pass, auth_url.as_str()])
        {
            log::warn!(
                "Failed to destroy fxacct {} with pass {}!",
                self.email,
                self.pass
            );
            log::warn!("   Error: {}", e);
        }
    }
}

pub struct TestClient {
    pub fxa: fxa_client::FirefoxAccount,
    pub test_acct: Arc<TestAccount>,
    // XXX do this more generically...
    pub logins_engine: PasswordEngine,
}

impl TestClient {
    pub fn new(acct: Arc<TestAccount>) -> Result<Self, failure::Error> {
        log::info!("Doing oauth flow!");

        let mut fxa = FirefoxAccount::with_config(acct.cfg.clone());
        let oauth_uri = fxa.begin_oauth_flow(&[SYNC_SCOPE], true)?;
        let auth_url = acct.cfg.auth_url()?;
        let redirected_to = run_helper_command(
            "oauth",
            &[&acct.email, &acct.pass, auth_url.as_str(), &oauth_uri],
        )?;

        log::info!("Helper command gave '{}'", redirected_to);

        let final_url = Url::parse(&redirected_to)?;
        let query_params = final_url
            .query_pairs()
            .into_owned()
            .collect::<HashMap<String, String>>();

        // should we be using the OAuthInfo this returns?
        fxa.complete_oauth_flow(&query_params["code"], &query_params["state"])?;
        log::info!("OAuth flow finished");

        Ok(Self {
            fxa,
            test_acct: acct,
            logins_engine: PasswordEngine::new_in_memory(None)?,
        })
    }

    pub fn data_for_sync(
        &mut self,
    ) -> Result<(Sync15StorageClientInit, KeyBundle), failure::Error> {
        // Allow overriding it via environment
        let tokenserver_url = option_env!("TOKENSERVER_URL")
            .map(|env_var| {
                // We hard error here even though we want to return a Result to provide a clearer
                // error for misconfiguration
                Ok(Url::parse(env_var)
                    .expect("Failed to parse TOKENSERVER_URL environment variable!"))
            })
            .unwrap_or_else(|| self.test_acct.cfg.token_server_endpoint_url())?;
        let token = self.fxa.get_access_token(SYNC_SCOPE)?;

        let key = token.key.as_ref().unwrap();

        let client_init = Sync15StorageClientInit {
            key_id: key.kid.clone(),
            access_token: token.token,
            tokenserver_url,
        };

        let root_sync_key = KeyBundle::from_ksync_base64(&key.k)?;

        Ok((client_init, root_sync_key))
    }

    pub fn fully_wipe_server(&mut self) -> Result<bool, failure::Error> {
        use sync15::client::SetupStorageClient;
        // XXX cludgey to use logins_engine here...
        let info = self.logins_engine.client_info.replace(None);
        let res = match &info {
            Some(info) => info.test_only_get_client().wipe_all_remote().map(|_| true),
            None => Ok(false),
        };
        self.logins_engine.client_info.replace(info);
        Ok(res?)
    }

    pub fn fully_reset_local_db(&mut self) -> Result<(), failure::Error> {
        // Not great...
        self.logins_engine = PasswordEngine::new_in_memory(None)?;
        Ok(())
    }
}

// Wipes the server using the first client that can manage it.
// We do this at the end of each test to avoid creating N accounts for N tests,
// and just creating 1 account per file containing tests.
// TODO: this probably shouldn't take a vec but whatever.
pub fn cleanup_server(clients: Vec<&mut TestClient>) -> Result<(), failure::Error> {
    log::info!("Cleaning up server after tests...");
    for c in clients {
        match c.fully_wipe_server() {
            Ok(true) => return Ok(()),
            Ok(false) => {
                log::info!("Can't wipe server (no client state). Hopefully this is intentional");
            }
            Err(e) => {
                log::warn!("Error when wiping server: {:?}", e);
            }
        }
    }
    failure::bail!("None of the clients managed to wipe the server!");
}

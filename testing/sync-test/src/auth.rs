/* Any copyright is dedicated to the Public Domain.
http://creativecommons.org/publicdomain/zero/1.0/ */

use crate::{restmail_helper, Opts};
use fxa_client::{self, Config as FxaConfig, FirefoxAccount, http_client, auth};
use logins::PasswordEngine;
use serde_json::json;
use std::sync::{Arc, Once};
use sync15::{KeyBundle, Sync15StorageClientInit};
use tabs::TabsEngine;
use url::Url;
use viaduct::{Request, Method};

pub const CLIENT_ID: &str = "3c49430b43dfba77"; // Hrm...
pub const SYNC_SCOPE: &str = "https://identity.mozilla.com/apps/oldsync";

// TODO: This is wrong for dev?
pub const REDIRECT_URI: &str = "https://stable.dev.lcip.org/oauth/success/3c49430b43dfba77";

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
    static HELPER_SETUP: Once = Once::new();
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
    pub no_delete: bool,
}

impl TestAccount {
    fn new(
        email: String,
        pass: String,
        cfg: FxaConfig,
        no_delete: bool,
    ) -> Result<Arc<TestAccount>, failure::Error> {
        log::info!("Creating temporary fx account");

        restmail_helper::clear_mailbox(&email);

        let create_endpoint = cfg.auth_url_path("v1/account/create").unwrap();
        let body = json!({
            "email": &email,
            "authPW": auth::auth_pwd(&email, &pass)
        });
        let req = Request::post(create_endpoint).json(&body).send().unwrap();
        let resp: serde_json::Value = req.json().unwrap();
        let uid = resp["uid"].as_str().unwrap();

        log::info!("POST /v1/account/create succeeded");
        log::info!("Autoverifying account on restmail... uid = {}", uid);

        let verification_email = restmail_helper::find_email(&email, |email| {
            email["headers"]["x-uid"] == uid && email["headers"]["x-template-name"] == "verify"
        });

        let verify_endpoint = cfg.auth_url_path("v1/recovery_email/verify_code").unwrap();
        let body = json!({
            "uid": &uid,
            "code": verification_email["headers"]["x-verify-code"].as_str().unwrap(),
        });
        let resp = Request::post(verify_endpoint).json(&body).send().unwrap();
        if !resp.is_success() {
            panic!("Failed to verify the account: {}", resp.text());
        }

        log::info!("Account created and verified!");
        Ok(Arc::new(TestAccount {
            email,
            pass,
            cfg,
            no_delete,
        }))
    }

    pub fn new_random(opts: &Opts) -> Result<Arc<TestAccount>, failure::Error> {
        use rand::prelude::*;
        let rng = thread_rng();
        let name = opts.force_username.clone().unwrap_or_else(|| {
            format!(
                "rust-login-sql-test--{}",
                rng.sample_iter(&rand::distributions::Alphanumeric)
                    .take(5)
                    .collect::<String>()
            )
        });
        // We should probably check this some other time, but whatever.
        assert!(
            !name.contains('@'),
            "--force-username passed an illegal username"
        );
        // Just use the username for the password in case we need to clean these
        // up easily later because of some issue.
        let password = name.clone();
        let email = format!("{}@restmail.net", name);
        Self::new(
            email,
            password,
            opts.fxa_stack.to_config(CLIENT_ID, REDIRECT_URI),
            opts.no_delete_account,
        )
    }

    pub fn get_account_keys(&self, key_fetch_token: &str) -> Result<Vec<u8>, fxa_client::error::Error> {
        let creds = auth::derive_hawk_credentials(key_fetch_token, "keyFetchToken", 96)?;
        let key_request_key = &creds["extra"].as_str().unwrap().as_bytes()[0..32];
        let more_creds = auth::derive_hkdf_sha256_key(key_request_key, &[0u8; 0], &auth::kw("account/keys"), 96);
        let resp_hmac_key = &more_creds[0..32];
        let resp_xor_key = &more_creds[32..96];
        let keys_url = self.cfg.auth_url_path("v1/account/keys").unwrap();
        let req = Request::get(keys_url).json(&creds);
        let resp: serde_json::Value = http_client::make_request(req)?.json()?;
        let bundle = hex::decode(&resp["bundle"].as_str().unwrap())?;
        auth::xored(resp_xor_key, &bundle[0..64])
    }
}

impl Drop for TestAccount {
    fn drop(&mut self) {
        if self.no_delete {
            log::info!("Cleanup was explicitly disabled, not deleting account");
            return;
        }
        log::info!("Cleaning up temporary firefox account");
        let destroy_endpoint = self.cfg.auth_url_path("v1/account/destroy").unwrap();
        let body = json!({
            "email": self.email,
            "authPW": auth::auth_pwd(&self.email, &self.pass)
        });
        let req = Request::post(destroy_endpoint).json(&body).send();
        match req {
            Ok(resp) => {
                if resp.is_success() {
                    log::info!("Account destroyed successfully!");
                    return;
                } else {
                    log::warn!("   Error: {}", resp.text());
                }
            }
            Err(e) => log::warn!("   Error: {}", e),
        }
        log::warn!(
            "Failed to destroy fxacct {} with pass {}!",
            self.email,
            self.pass
        );
    }
}

pub struct TestClient {
    pub fxa: fxa_client::FirefoxAccount,
    pub test_acct: Arc<TestAccount>,
    // XXX do this more generically...
    pub logins_engine: PasswordEngine,
    pub tabs_engine: TabsEngine,
}

impl TestClient {
    pub fn new(acct: Arc<TestAccount>) -> Result<Self, failure::Error> {
        log::info!("Doing oauth flow!");

        let mut fxa = FirefoxAccount::with_config(acct.cfg.clone());
        let oauth_uri = fxa.begin_oauth_flow(&[SYNC_SCOPE])?;
        let auth_url = acct.cfg.auth_url()?;

        let login_endpoint = acct.cfg.auth_url_path("v1/account/login?keys=true").unwrap();
        let body = json!({
            "email": &acct.email,
            "authPW": auth::auth_pwd(&acct.email, &acct.pass),
            "service": &acct.cfg.client_id,
            "verificationMethod": "email-otp",
        });
        let req = Request::post(login_endpoint).json(&body);
        let resp: serde_json::Value = http_client::make_request(req)?.json()?;

        log::info!("POST /v1/account/login succeeded");
        let session_token = resp["sessionToken"].as_str().unwrap();
        let key_fetch_token = resp["keyFetchToken"].as_str().unwrap();

        let verified: bool = resp["verified"].as_bool().unwrap();
        let uid = resp["uid"].as_str().unwrap();
        let auth_key = http_client::derive_auth_key_from_session_token(&session_token)?;

        if !verified {
            log::info!("Need to do sign-in confirmation, attempting to do so through restmail...");
            let verification_email = restmail_helper::find_email(&acct.email, |email| {
                email["headers"]["x-template-name"] == "verifyLoginCode"
            });
            let verify_endpoint = acct.cfg.auth_url_path("v1/session/verify_code").unwrap();
            let body = json!({
                "code": verification_email["headers"]["x-signin-verify-code"].as_str().unwrap(),
            });

            let req = http_client::HawkRequestBuilder::new(Method::Post, verify_endpoint, &auth_key).body(body).build()?;
            let _resp = http_client::make_request(req)?;
        }
        let code_challenge = auth::random_code_challenge()?;
        let state = auth::random_state()?;
        // kA: 0..32, wrapKB: 32..64
        let acct_keys = acct.get_account_keys(key_fetch_token)?;
        let wrap_kb = &acct_keys[32..64];
        let sync_key = auth::derive_sync_key(&acct.email, &acct.pass, wrap_kb)?;

        let auth_endpoint = acct.cfg.auth_url_path("v1/oauth/authorization").unwrap();

        let body = json!({
            "access_type": "offline",
            "client_id": &acct.cfg.client_id,
            "code_challenge": &code_challenge,
            "code_challenge_method": "S256",
            "keys_jwe": std::str::from_utf8(&sync_key)?,
            "state": &state,
        });
        // TODO: hawk auth
        let req = http_client::HawkRequestBuilder::new(Method::Post, auth_endpoint, &auth_key).body(body).build()?;
        let resp: serde_json::Value = http_client::make_request(req)?.json().unwrap();
        let code = resp["code"].as_str().unwrap();
        let state = resp["state"].as_str().unwrap();

        fxa.complete_oauth_flow(code, state)?;
        log::info!("OAuth flow finished");

        fxa.initialize_device("Testing Device", fxa_client::device::Type::Desktop, &[])?;

        Ok(Self {
            fxa,
            test_acct: acct,
            logins_engine: PasswordEngine::new_in_memory(None)?,
            tabs_engine: TabsEngine::new(),
        })
    }

    pub fn data_for_sync(
        &mut self,
    ) -> Result<(Sync15StorageClientInit, KeyBundle, String), failure::Error> {
        // Allow overriding it via environment
        let tokenserver_url = option_env!("TOKENSERVER_URL")
            .map(|env_var| {
                // We hard error here even though we want to return a Result to provide a clearer
                // error for misconfiguration
                Ok(Url::parse(env_var)
                    .expect("Failed to parse TOKENSERVER_URL environment variable!"))
            })
            .unwrap_or_else(|| self.test_acct.cfg.token_server_endpoint_url())?;
        let token = self.fxa.get_access_token(SYNC_SCOPE, None)?;

        let key = token.key.as_ref().unwrap();

        let client_init = Sync15StorageClientInit {
            key_id: key.kid.clone(),
            access_token: token.token,
            tokenserver_url,
        };

        let root_sync_key = KeyBundle::from_ksync_base64(&key.k)?;

        let device_id = self.fxa.get_current_device_id()?;

        Ok((client_init, root_sync_key, device_id))
    }

    pub fn fully_wipe_server(&mut self) -> Result<(), failure::Error> {
        use sync15::{SetupStorageClient, Sync15StorageClient};
        let client_init = self.data_for_sync()?.0;
        Sync15StorageClient::new(client_init)?.wipe_all_remote()?;
        Ok(())
    }

    pub fn fully_reset_local_db(&mut self) -> Result<(), failure::Error> {
        // Not great...
        self.logins_engine = PasswordEngine::new_in_memory(None)?;
        self.tabs_engine = TabsEngine::new();
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
            Ok(()) => return Ok(()),
            Err(e) => {
                log::warn!("Error when wiping server: {:?}", e);
                // and I guess we try again, even though there's no reason
                // the next client should succeed here.
            }
        }
    }
    failure::bail!("None of the clients managed to wipe the server!");
}

pub struct TestUser {
    pub account: Arc<TestAccount>,
    pub clients: Vec<TestClient>,
}

impl TestUser {
    fn new_random(opts: &Opts, client_count: usize) -> Result<Self, failure::Error> {
        log::info!("Creating test account with {} clients", client_count);

        let account = TestAccount::new_random(&opts)?;
        let mut clients = Vec::with_capacity(client_count);

        for c in 0..client_count {
            log::info!("Creating test client {}", c);
            clients.push(TestClient::new(account.clone())?);
        }
        Ok(Self { account, clients })
    }

    pub fn new(opts: &Opts, client_count: usize) -> Result<TestUser, failure::Error> {
        if opts.oauth_retries > 0 && opts.no_delete_account {
            failure::bail!(
                "Illegal option combination: oauth-retries is nonzero \
                 and no-delete-account is specified."
            );
        }
        if opts.helper_debug {
            std::env::set_var("DEBUG", "nightmare");
            std::env::set_var("HELPER_SHOW_BROWSER", "1");
        }
        for attempt in 0..=opts.oauth_retries {
            log::info!("Creating test user (attempt {})", attempt);
            match TestUser::new_random(opts, client_count) {
                Ok(user) => {
                    log::info!("Created test user (attempt {})", attempt);
                    return Ok(user);
                }
                Err(e) => {
                    if attempt == opts.oauth_retries {
                        log::error!("Failed to create test user (attempt {}): {:?}", attempt, e);
                        return Err(e);
                    }
                    log::warn!("Failed to create test user (attempt {}): {}", attempt, e);
                    if opts.oauth_delay_time > 0 {
                        let delay = opts.oauth_delay_time + attempt * opts.oauth_retry_backoff;
                        log::info!(
                            "Retrying after {} ms (attempt {} => {})",
                            delay,
                            attempt,
                            attempt + 1
                        );
                        std::thread::sleep(std::time::Duration::from_millis(delay as u64));
                    }
                }
            }
        }
        // Above loop always either hits the `return Err(e)` or `return Ok(user);` cases
        unreachable!();
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum FxaConfigUrl {
    StableDev,
    Stage,
    Release,
    Custom(url::Url),
}

impl FxaConfigUrl {
    pub fn to_config(&self, client_id: &str, redirect: &str) -> FxaConfig {
        match self {
            FxaConfigUrl::StableDev => FxaConfig::stable_dev(client_id, redirect),
            FxaConfigUrl::Stage => FxaConfig::stage_dev(client_id, redirect),
            FxaConfigUrl::Release => FxaConfig::release(client_id, redirect),
            FxaConfigUrl::Custom(url) => FxaConfig::new(url.as_str(), client_id, redirect),
        }
    }
}

// Required for arg parsing
impl std::str::FromStr for FxaConfigUrl {
    type Err = failure::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "release" => FxaConfigUrl::Release,
            "stage" => FxaConfigUrl::Stage,
            "stable-dev" => FxaConfigUrl::StableDev,
            s if s.contains(':') => FxaConfigUrl::Custom(url::Url::parse(s)?),
            _ => {
                failure::bail!(
                    "Illegal fxa-stack option '{}', not a url nor a known alias",
                    s
                );
            }
        })
    }
}

/* Any copyright is dedicated to the Public Domain.
http://creativecommons.org/publicdomain/zero/1.0/ */

use crate::Opts;
use anyhow::Result;
use fxa_client::{self, auth, Config as FxaConfig, FirefoxAccount};
use logins::PasswordEngine;
use serde_json::json;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;
use sync15::{KeyBundle, Sync15StorageClientInit};
use tabs::TabsEngine;
use url::Url;
use viaduct::Request;

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

// It's important that this doesn't implement Clone! (It destroys it's temporary fxaccount on drop)
#[derive(Debug)]
pub struct TestAccount {
    pub email: String,
    pub pass: String,
    pub cfg: FxaConfig,
    pub no_delete: bool,
    pub session_token: String,
    pub k_sync: Vec<u8>,
    pub xcs: Vec<u8>,
}

impl TestAccount {
    fn new(
        email: String,
        pass: String,
        cfg: FxaConfig,
        no_delete: bool,
    ) -> Result<Arc<TestAccount>> {
        log::info!("Creating temporary fx account");

        restmail_client::clear_mailbox(&email).unwrap();

        let create_endpoint = cfg.auth_url_path("v1/account/create?keys=true").unwrap();
        let body = json!({
            "email": &email,
            "authPW": auth::auth_pwd(&email, &pass)?,
            "service": &cfg.client_id,
        });
        let req = Request::post(create_endpoint).json(&body).send()?;
        let resp: serde_json::Value = req.json()?;
        let uid = resp["uid"]
            .as_str()
            .ok_or_else(|| anyhow::Error::msg("No Uid"))?;
        let session_token = resp["sessionToken"]
            .as_str()
            .ok_or_else(|| anyhow::Error::msg("No session Token"))?;
        let key_fetch_token = resp["keyFetchToken"]
            .as_str()
            .ok_or_else(|| anyhow::Error::msg("No Key fetch token"))?;
        log::info!("POST /v1/account/create succeeded");

        log::info!("Autoverifying account on restmail... uid = {}", uid);
        Self::verify_account(&email, &cfg, &uid)?;
        let (sync_key, xcs_key) = auth::get_sync_keys(&cfg, &key_fetch_token, &email, &pass)?;
        log::info!("Account created and verified!");

        Ok(Arc::new(TestAccount {
            email,
            pass,
            cfg,
            no_delete,
            session_token: session_token.to_string(),
            k_sync: sync_key,
            xcs: xcs_key,
        }))
    }

    pub fn new_random(opts: &Opts) -> Result<Arc<TestAccount>> {
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

    fn verify_account(email_in: &str, config: &FxaConfig, uid: &str) -> Result<()> {
        let verification_email = restmail_client::find_email(
            email_in,
            |email| {
                email["headers"]["x-uid"] == uid && email["headers"]["x-template-name"] == "verify"
            },
            10,
        )
        .unwrap();
        let code = verification_email["headers"]["x-verify-code"]
            .as_str()
            .unwrap();
        log::info!("Code is: {}", code);
        let body = json!({
            "uid": uid,
            "code": verification_email["headers"]["x-verify-code"].as_str().unwrap(),
        });
        let resp = auth::send_verification(&config, body).unwrap();
        if !resp.is_success() {
            log::warn!(
                "Error verifying account: {}",
                resp.json::<serde_json::Value>().unwrap()
            );
            anyhow::bail!("Unable to verify account!");
        }
        Ok(())
    }

    pub fn execute_oauth_flow(&self, oauth_url: &str) -> Result<String> {
        let url = Url::parse(oauth_url)?;
        let auth_key = auth::derive_auth_key_from_session_token(&self.session_token)?;
        let query_map: HashMap<String, String> = url.query_pairs().into_owned().collect();
        let jwk_base_64 = query_map.get("keys_jwk").unwrap();
        let decoded = base64::decode(&jwk_base_64).unwrap();
        let jwk = std::str::from_utf8(&decoded)?;
        let scope = query_map.get("scope").unwrap();
        let client_id = query_map.get("client_id").unwrap();
        let state = query_map.get("state").unwrap();
        let code_challenge = query_map.get("code_challenge").unwrap();
        let code_challenge_method = query_map.get("code_challenge_method").unwrap();
        let keys_jwe = auth::create_keys_jwe(
            &client_id,
            &scope,
            &jwk,
            &auth_key,
            &self.cfg,
            (&self.k_sync, &self.xcs),
        )?;
        let auth_params = auth::AuthorizationRequestParameters {
            client_id: client_id.clone(),
            code_challenge: Some(code_challenge.clone()),
            code_challenge_method: Some(code_challenge_method.clone()),
            scope: scope.clone(),
            keys_jwe: Some(keys_jwe),
            state: state.clone(),
            access_type: "offline".to_string(),
        };
        auth::send_authorization_request(&self.cfg, auth_params, &auth_key)
    }

    fn execute_oauth_pair_flow(&self, oauth_uri: &str) -> Result<(String, String)> {
        let url = Url::parse(&oauth_uri)?;
        let auth_params = auth::AuthorizationParameters::try_from(url)?;
        let scoped_keys = auth::get_scoped_keys(
            &auth_params.scope,
            &auth_params.client_id,
            &auth::derive_auth_key_from_session_token(&self.session_token)?,
            &self.cfg,
            (&self.k_sync, &self.xcs),
        )?;
        // Setup authority account that is logged in and has the appropriate scoped keys
        let fxa = FirefoxAccount::new_logged_in(self.cfg.clone(), &self.session_token, scoped_keys);
        // Use the logged in client to generate the oauth code for
        // a different client
        let state = auth_params.state.clone();
        let code = fxa.authorize_code_using_session_token(auth_params)?;
        Ok((code, state))
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
            "authPW": auth::auth_pwd(&self.email, &self.pass).unwrap()
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
    pub fn new(acct: Arc<TestAccount>) -> Result<Self> {
        log::info!("Doing oauth flow!");
        let mut fxa = FirefoxAccount::with_config(acct.cfg.clone());
        // We either authenticate using the normal oauth_flow
        // Or we use a pairing flow with a logged in account
        // Both should work fine in executing the oauth flow
        let (code, state) = if rand::random() {
            let pairing_url = acct.cfg.authorization_endpoint().unwrap();
            let pairing_url =
                fxa.begin_pairing_flow(pairing_url.as_str(), &[SYNC_SCOPE], "integration_test")?;
            acct.execute_oauth_pair_flow(&pairing_url)?
        } else {
            let oauth_uri = fxa.begin_oauth_flow(&[SYNC_SCOPE], "integration_test")?;
            let redirect_uri = acct.execute_oauth_flow(&oauth_uri)?;
            let redirect_uri = Url::parse(&redirect_uri)?;
            let query_params = redirect_uri
                .query_pairs()
                .into_owned()
                .collect::<HashMap<String, String>>();
            (query_params["code"].clone(), query_params["state"].clone())
        };
        // should we be using the OAuthInfo this returns?
        fxa.complete_oauth_flow(&code, &state)?;
        log::info!("OAuth flow finished");

        fxa.initialize_device("Testing Device", fxa_client::device::Type::Desktop, &[])?;

        Ok(Self {
            fxa,
            test_acct: acct,
            logins_engine: PasswordEngine::new_in_memory(None)?,
            tabs_engine: TabsEngine::new(),
        })
    }

    pub fn data_for_sync(&mut self) -> Result<(Sync15StorageClientInit, KeyBundle, String)> {
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

    pub fn fully_wipe_server(&mut self) -> Result<()> {
        use sync15::{SetupStorageClient, Sync15StorageClient};
        let client_init = self.data_for_sync()?.0;
        Sync15StorageClient::new(client_init)?.wipe_all_remote()?;
        Ok(())
    }

    pub fn fully_reset_local_db(&mut self) -> Result<()> {
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
pub fn cleanup_server(clients: Vec<&mut TestClient>) -> Result<()> {
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
    anyhow::bail!("None of the clients managed to wipe the server!");
}

pub struct TestUser {
    pub account: Arc<TestAccount>,
    pub clients: Vec<TestClient>,
}

impl TestUser {
    fn new_random(opts: &Opts, client_count: usize) -> Result<Self> {
        log::info!("Creating test account with {} clients", client_count);

        let account = TestAccount::new_random(&opts)?;
        let mut clients = Vec::with_capacity(client_count);

        for c in 0..client_count {
            log::info!("Creating test client {}", c);
            clients.push(TestClient::new(account.clone())?);
        }
        Ok(Self { account, clients })
    }

    pub fn new(opts: &Opts, client_count: usize) -> Result<TestUser> {
        if opts.oauth_retries > 0 && opts.no_delete_account {
            anyhow::bail!(
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
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "release" => FxaConfigUrl::Release,
            "stage" => FxaConfigUrl::Stage,
            "stable-dev" => FxaConfigUrl::StableDev,
            s if s.contains(':') => FxaConfigUrl::Custom(url::Url::parse(s)?),
            _ => {
                anyhow::bail!(
                    "Illegal fxa-stack option '{}', not a url nor a known alias",
                    s
                );
            }
        })
    }
}

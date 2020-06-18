/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use cli_support::prompt::prompt_string;
use dialoguer::Select;
use fxa_client::{auth, device, pairing_channel, Config, FirefoxAccount, IncomingDeviceCommand};
use serde_json::json;
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    sync::{Arc, Mutex},
    thread, time,
};
use url::Url;
use viaduct::Request;

static CREDENTIALS_PATH: &str = "credentials.json";
static CONTENT_SERVER: &str = "https://stable.dev.lcip.org/";
static CLIENT_ID: &str = "a2270f727f45f648";
static REDIRECT_URI: &str = "https://stable.dev.lcip.org/oauth/success/a2270f727f45f648";
static SCOPES: &[&str] = &["profile", "https://identity.mozilla.com/apps/oldsync"];
static DEFAULT_DEVICE_NAME: &str = "Bobo device";

use anyhow::Result;

fn load_fxa_creds() -> Result<FirefoxAccount> {
    let mut file = fs::File::open(CREDENTIALS_PATH)?;
    let mut s = String::new();
    file.read_to_string(&mut s)?;
    Ok(FirefoxAccount::from_json(&s)?)
}

fn load_or_create_fxa_creds(cfg: Config) -> Result<FirefoxAccount> {
    let acct = load_fxa_creds().or_else(|_e| create_fxa_creds(cfg))?;
    persist_fxa_state(&acct);
    Ok(acct)
}

fn persist_fxa_state(acct: &FirefoxAccount) {
    let json = acct.to_json().unwrap();
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .truncate(true)
        .create(true)
        .open(CREDENTIALS_PATH)
        .unwrap();
    write!(file, "{}", json).unwrap();
    file.flush().unwrap();
}

fn create_fxa_creds(cfg: Config) -> Result<FirefoxAccount> {
    let mut acct = FirefoxAccount::with_config(cfg.clone());
    let oauth_uri = acct.begin_oauth_flow(&SCOPES, "device_api_example")?;
    let email = prompt_string("email").unwrap();
    let password = dialoguer::Password::new()
        .with_prompt("Enter password")
        .interact()
        .unwrap();
    let login_endpoint = cfg.auth_url_path("v1/account/login?keys=true").unwrap();
    let body = json!({
        "email": &email,
        "authPW": auth::auth_pwd(&email, &password)?,
        "service": &cfg.client_id,
        "verificationMethod": "email-otp",
    });
    let req = Request::post(login_endpoint).json(&body).send()?;
    let resp: serde_json::Value = req.json()?;
    let session_token = resp["sessionToken"]
        .as_str()
        .ok_or_else(|| anyhow::Error::msg("No session Token"))?;
    let key_fetch_token = resp["keyFetchToken"]
        .as_str()
        .ok_or_else(|| anyhow::Error::msg("No Key fetch token"))?;
    log::info!("POST /v1/account/create succeeded");
    let verified = resp["verified"]
        .as_bool()
        .ok_or_else(|| anyhow::Error::msg("No verified"))?;
    if !verified {
        let verification_code = prompt_string("Verification code").unwrap();
        let body = json!({ "code": verification_code });
        auth::verify_session(&cfg, body, session_token)?;
    }
    let (sync_key, xcs_key) = auth::get_sync_keys(&cfg, &key_fetch_token, &email, &password)?;
    let redirect_uri = execute_oauth_flow(&cfg, &oauth_uri, session_token, (&sync_key, &xcs_key))?;
    let redirect_uri = Url::parse(&redirect_uri).unwrap();
    let query_params: HashMap<_, _> = redirect_uri.query_pairs().into_owned().collect();
    let code = &query_params["code"];
    let state = &query_params["state"];
    acct.complete_oauth_flow(&code, &state).unwrap();
    acct.set_session_token(session_token);
    persist_fxa_state(&acct);
    Ok(acct)
}

fn execute_oauth_flow(
    cfg: &Config,
    oauth_url: &str,
    session_token: &str,
    sync_keys: (&[u8], &[u8]),
) -> Result<String> {
    let url = Url::parse(oauth_url)?;
    let auth_key = auth::derive_auth_key_from_session_token(session_token)?;
    let query_map: HashMap<String, String> = url.query_pairs().into_owned().collect();
    let jwk_base_64 = query_map.get("keys_jwk").unwrap();
    let decoded = base64::decode(&jwk_base_64).unwrap();
    let jwk = std::str::from_utf8(&decoded)?;
    let scope = query_map.get("scope").unwrap();
    let client_id = query_map.get("client_id").unwrap();
    let state = query_map.get("state").unwrap();
    let code_challenge = query_map.get("code_challenge").unwrap();
    let code_challenge_method = query_map.get("code_challenge_method").unwrap();
    let keys_jwe = auth::create_keys_jwe(&client_id, &scope, &jwk, &auth_key, cfg, sync_keys)?;
    let auth_params = auth::AuthorizationRequestParameters {
        client_id: client_id.clone(),
        code_challenge: Some(code_challenge.clone()),
        code_challenge_method: Some(code_challenge_method.clone()),
        scope: scope.clone(),
        keys_jwe: Some(keys_jwe),
        state: state.clone(),
        access_type: "offline".to_string(),
    };
    auth::send_authorization_request(cfg, auth_params, &auth_key)
}

fn main() -> Result<()> {
    viaduct_reqwest::use_reqwest_backend();
    let cfg = Config::new(CONTENT_SERVER, CLIENT_ID, REDIRECT_URI);
    let mut acct = load_or_create_fxa_creds(cfg)?;

    // Make sure the device and the send-tab command are registered.
    acct.initialize_device(
        DEFAULT_DEVICE_NAME,
        device::Type::Desktop,
        &[device::Capability::SendTab],
    )
    .unwrap();
    persist_fxa_state(&acct);

    let acct: Arc<Mutex<FirefoxAccount>> = Arc::new(Mutex::new(acct));
    {
        let acct = acct.clone();
        thread::spawn(move || {
            loop {
                let evts = acct
                    .lock()
                    .unwrap()
                    .poll_device_commands()
                    .unwrap_or_else(|_| vec![]); // Ignore 404 errors for now.
                persist_fxa_state(&acct.lock().unwrap());
                for e in evts {
                    match e {
                        IncomingDeviceCommand::TabReceived { sender, payload } => {
                            let tab = &payload.entries[0];
                            match sender {
                                Some(ref d) => {
                                    println!("Tab received from {}: {}", d.display_name, tab.url)
                                }
                                None => println!("Tab received: {}", tab.url),
                            };
                            webbrowser::open(&tab.url).unwrap();
                        }
                    }
                }
                thread::sleep(time::Duration::from_secs(1));
            }
        });
    }

    loop {
        println!("Main menu:");
        let mut main_menu = Select::new();
        main_menu.items(&["Set Display Name", "Send a Tab", "Pair device!", "Quit"]);
        main_menu.default(0);
        let main_menu_selection = main_menu.interact().unwrap();

        match main_menu_selection {
            0 => {
                let new_name: String = prompt_string("New display name").unwrap();
                // Set device display name
                acct.lock().unwrap().set_device_name(&new_name).unwrap();
                println!("Display name set to: {}", new_name);
            }
            1 => {
                let devices = acct.lock().unwrap().get_devices(false).unwrap();
                let devices_names: Vec<String> =
                    devices.iter().map(|i| i.display_name.clone()).collect();
                let mut targets_menu = Select::new();
                targets_menu.default(0);
                let devices_names_refs: Vec<&str> =
                    devices_names.iter().map(AsRef::as_ref).collect();
                targets_menu.items(&devices_names_refs);
                println!("Choose a send-tab target:");
                let selection = targets_menu.interact().unwrap();
                let target = &devices[selection];

                // Payload
                let title: String = prompt_string("Title").unwrap();
                let url: String = prompt_string("URL").unwrap();
                acct.lock()
                    .unwrap()
                    .send_tab(&target.id, &title, &url)
                    .unwrap();
                println!("Tab sent!");
            }
            2 => {
                pairing_channel::run_server(
                    &acct,
                    "wss://dev.channelserver.nonprod.cloudops.mozgcp.net/v1/ws/",
                )?;
            }
            3 => ::std::process::exit(0),
            _ => panic!("Invalid choice!"),
        }
    }
}

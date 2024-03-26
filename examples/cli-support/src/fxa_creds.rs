/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// Utilities for command-line utilities which want to use fxa credentials.
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
};

use anyhow::Result;
use url::Url;

// This crate awkardly uses some internal implementation details of the fxa-client crate,
// because we haven't worked on exposing those test-only features via UniFFI.
use fxa_client::{AccessTokenInfo, FirefoxAccount, FxaConfig, FxaError};
use sync15::client::Sync15StorageClientInit;
use sync15::KeyBundle;

use crate::prompt::prompt_string;

// Defaults - not clear they are the best option, but they are a currently
// working option.
const CLIENT_ID: &str = "3c49430b43dfba77";
const REDIRECT_URI: &str = "https://accounts.firefox.com/oauth/success/3c49430b43dfba77";
pub const SYNC_SCOPE: &str = "https://identity.mozilla.com/apps/oldsync";
pub const SESSION_SCOPE: &str = "https://identity.mozilla.com/tokens/session";

fn load_fxa_creds(path: &str) -> Result<FirefoxAccount> {
    let mut file = fs::File::open(path)?;
    let mut s = String::new();
    file.read_to_string(&mut s)?;
    Ok(FirefoxAccount::from_json(&s)?)
}

fn load_or_create_fxa_creds(path: &str, cfg: FxaConfig, scopes: &[&str]) -> Result<FirefoxAccount> {
    load_fxa_creds(path).or_else(|e| {
        log::info!(
            "Failed to load existing FxA credentials from {:?} (error: {}), launching OAuth flow",
            path,
            e
        );
        create_fxa_creds(path, cfg, scopes)
    })
}

fn create_fxa_creds(path: &str, cfg: FxaConfig, scopes: &[&str]) -> Result<FirefoxAccount> {
    let acct = FirefoxAccount::new(cfg);
    handle_oauth_flow(path, &acct, scopes)?;
    Ok(acct)
}

fn handle_oauth_flow(path: &str, acct: &FirefoxAccount, scopes: &[&str]) -> Result<()> {
    let oauth_uri = acct.begin_oauth_flow(scopes, "fxa_creds")?;

    if webbrowser::open(oauth_uri.as_ref()).is_err() {
        log::warn!("Failed to open a web browser D:");
        println!("Please visit this URL, sign in, and then copy-paste the final URL below.");
        println!("\n    {}\n", oauth_uri);
    } else {
        println!("Please paste the final URL below:\n");
    }

    let final_url = url::Url::parse(&prompt_string("Final URL").unwrap_or_default())?;
    let query_params = final_url
        .query_pairs()
        .into_owned()
        .collect::<HashMap<String, String>>();

    acct.complete_oauth_flow(&query_params["code"], &query_params["state"])?;
    // Device registration.
    acct.initialize_device("CLI Device", sync15::DeviceType::Desktop, vec![])?;
    let mut file = fs::File::create(path)?;
    write!(file, "{}", acct.to_json()?)?;
    file.flush()?;
    Ok(())
}

// Our public functions. It would be awesome if we could somehow integrate
// better with clap, so we could automagically support various args (such as
// the config to use or filenames to read), but this will do for now.
pub fn get_default_fxa_config() -> FxaConfig {
    FxaConfig::release(CLIENT_ID, REDIRECT_URI)
}

pub fn get_account_and_token(
    config: FxaConfig,
    cred_file: &str,
    scopes: &[&str],
) -> Result<(FirefoxAccount, AccessTokenInfo)> {
    // TODO: we should probably set a persist callback on acct?
    let acct = load_or_create_fxa_creds(cred_file, config.clone(), scopes)?;
    // `scope` could be a param, but I can't see it changing.
    match acct.get_access_token(SYNC_SCOPE, None) {
        Ok(t) => Ok((acct, t)),
        Err(e) => {
            match e {
                // We can retry an auth error.
                FxaError::Authentication => {
                    println!("Saw an auth error using stored credentials - attempting to re-authenticate");
                    println!("If fails, consider deleting {cred_file} to start from scratch");
                    handle_oauth_flow(cred_file, &acct, scopes)?;
                    let token = acct.get_access_token(SYNC_SCOPE, None)?;
                    Ok((acct, token))
                }
                _ => Err(e.into()),
            }
        }
    }
}

pub fn get_cli_fxa(config: FxaConfig, cred_file: &str, scopes: &[&str]) -> Result<CliFxa> {
    let (account, token_info) = match get_account_and_token(config, cred_file, scopes) {
        Ok(v) => v,
        Err(e) => anyhow::bail!("Failed to use saved credentials. {}", e),
    };
    let tokenserver_url = Url::parse(&account.get_token_server_endpoint_url()?)?;

    let client_init = Sync15StorageClientInit {
        key_id: token_info.key.as_ref().unwrap().kid.clone(),
        access_token: token_info.token.clone(),
        tokenserver_url: tokenserver_url.clone(),
    };

    Ok(CliFxa {
        account,
        client_init,
        tokenserver_url,
        token_info,
    })
}

pub struct CliFxa {
    pub account: FirefoxAccount,
    pub client_init: Sync15StorageClientInit,
    pub tokenserver_url: Url,
    pub token_info: AccessTokenInfo,
}

impl CliFxa {
    // A helper for consumers who use this with the sync manager.
    pub fn as_auth_info(&self) -> sync_manager::SyncAuthInfo {
        let scoped_key = self.token_info.key.as_ref().unwrap();
        sync_manager::SyncAuthInfo {
            kid: scoped_key.kid.clone(),
            sync_key: scoped_key.k.clone(),
            fxa_access_token: self.token_info.token.clone(),
            tokenserver_url: self.tokenserver_url.to_string(),
        }
    }

    // A helper for consumers who use this directly with sync15
    pub fn as_key_bundle(&self) -> Result<KeyBundle> {
        let scoped_key = self.token_info.key.as_ref().unwrap();
        Ok(KeyBundle::from_ksync_bytes(&scoped_key.key_bytes()?)?)
    }
}

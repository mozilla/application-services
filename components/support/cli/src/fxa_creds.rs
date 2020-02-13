/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Utilities for command-line utilities which want to use fxa credentials.

use crate::prompt::prompt_string;
use fxa_client::{error, AccessTokenInfo, Config, FirefoxAccount};
use std::collections::HashMap;
use std::path::Path;
use std::{
    fs,
    io::{Read, Write},
};
use sync15::{KeyBundle, Sync15StorageClientInit};
use url::Url;
use webbrowser;
pub mod auto_restmail;
type Result<T> = std::result::Result<T, failure::Error>;

// Defaults - not clear they are the best option, but they are a currently
// working option.
const CLIENT_ID: &str = "e7ce535d93522896";
const REDIRECT_URI: &str = "https://lockbox.firefox.com/fxa/android-redirect.html";
const SYNC_SCOPE: &str = "https://identity.mozilla.com/apps/oldsync";

fn load_fxa_creds<P: ?Sized + AsRef<Path>>(path: &P) -> Result<FirefoxAccount> {
    let mut file = fs::File::open(path)?;
    let mut s = String::new();
    file.read_to_string(&mut s)?;
    Ok(FirefoxAccount::from_json(&s)?)
}

fn load_or_create_fxa_creds<P: ?Sized + AsRef<Path>>(
    path: &P,
    cfg: Config,
) -> Result<FirefoxAccount> {
    load_fxa_creds(path).or_else(|e| {
        log::info!(
            "Failed to load existing FxA credentials from {:?} (error: {}), launching OAuth flow",
            path.as_ref(),
            e
        );
        create_fxa_creds(path, cfg)
    })
}

fn create_fxa_creds<P: ?Sized + AsRef<Path>>(path: &P, cfg: Config) -> Result<FirefoxAccount> {
    let mut acct = FirefoxAccount::with_config(cfg);
    let oauth_uri = acct.begin_oauth_flow(&[SYNC_SCOPE])?;

    if webbrowser::open(&oauth_uri.as_ref()).is_err() {
        log::warn!("Failed to open a web browser D:");
        println!("Please visit this URL, sign in, and then copy-paste the final URL below.");
        println!("\n    {}\n", oauth_uri);
    } else {
        println!("Please paste the final URL below:\n");
    }

    let final_url = url::Url::parse(&prompt_string("Final URL").unwrap_or_default())?;
    complete_oauth(&mut acct, path.as_ref(), &final_url)?;

    Ok(acct)
}

fn complete_oauth(acct: &mut FirefoxAccount, creds_path: &Path, final_url: &Url) -> Result<()> {
    let query_params = final_url
        .query_pairs()
        .into_owned()
        .collect::<HashMap<String, String>>();

    acct.complete_oauth_flow(&query_params["code"], &query_params["state"])?;
    // Device registration.
    acct.initialize_device("CLI Device", fxa_client::device::Type::Desktop, &[])?;
    let mut file = fs::File::create(creds_path)?;
    write!(file, "{}", acct.to_json()?)?;
    file.flush()?;
    Ok(())
}

// Our public functions. It would be awesome if we could somehow integrate
// better with clap, so we could automagically support various args (such as
// the config to use or filenames to read), but this will do for now.
pub fn get_default_fxa_config() -> Config {
    Config::release(CLIENT_ID, REDIRECT_URI)
}
fn get_account_and_token<P: ?Sized + AsRef<Path>>(
    config: Config,
    cred_file: &P,
) -> Result<(FirefoxAccount, AccessTokenInfo)> {
    // TODO: we should probably set a persist callback on acct?
    let mut acct = load_or_create_fxa_creds(cred_file, config.clone())?;
    // `scope` could be a param, but I can't see it changing.
    match acct.get_access_token(SYNC_SCOPE) {
        Ok(t) => Ok((acct, t)),
        Err(e) => {
            match e.kind() {
                // We can retry an auth error.
                error::ErrorKind::RemoteError { code: 401, .. } => {
                    println!("Saw an auth error using stored credentials - recreating them...");
                    acct = create_fxa_creds(cred_file, config)?;
                    let token = acct.get_access_token(SYNC_SCOPE)?;
                    Ok((acct, token))
                }
                _ => Err(e.into()),
            }
        }
    }
}

pub fn get_cli_fxa<P: ?Sized + AsRef<Path>>(config: Config, cred_file: &P) -> Result<CliFxa> {
    let tokenserver_url = config.token_server_endpoint_url()?;
    let (acct, token_info) = match get_account_and_token(config, cred_file) {
        Ok(v) => v,
        Err(e) => failure::bail!("Failed to use saved credentials. {}", e),
    };
    let key = token_info.key.unwrap();

    let client_init = Sync15StorageClientInit {
        key_id: key.kid.clone(),
        access_token: token_info.token.clone(),
        tokenserver_url: tokenserver_url.clone(),
    };
    let root_sync_key = KeyBundle::from_ksync_bytes(&key.key_bytes()?)?;

    Ok(CliFxa {
        account: acct,
        client_init,
        tokenserver_url,
        root_sync_key,
    })
}

pub struct CliFxa {
    pub account: FirefoxAccount,
    pub client_init: Sync15StorageClientInit,
    pub tokenserver_url: Url,
    pub root_sync_key: KeyBundle,
}

impl CliFxa {
    fn new(mut acct: FirefoxAccount, cfg: &Config, tokenserver_url: Option<Url>) -> Result<Self> {
        // `scope` could be a param, but I can't see it changing.
        let token_info: AccessTokenInfo = match acct.get_access_token(SYNC_SCOPE) {
            Ok(t) => t,
            Err(e) => {
                panic!("No creds - run some other tool to set them up. {}", e);
            }
        };
        let key = token_info.key.unwrap();
        let tokenserver_url = tokenserver_url
            .map(Ok)
            .unwrap_or_else(|| cfg.token_server_endpoint_url())?;
        let client_init = Sync15StorageClientInit {
            key_id: key.kid.clone(),
            access_token: token_info.token.clone(),
            tokenserver_url: tokenserver_url.clone(),
        };
        let root_sync_key = KeyBundle::from_ksync_bytes(&key.key_bytes()?)?;

        Ok(CliFxa {
            account: acct,
            client_init,
            tokenserver_url,
            root_sync_key,
        })
    }
}

pub const STABLE_DEV_CLIENT_ID: &str = "3c49430b43dfba77"; // Hrm...
pub const STABLE_DEV_REDIRECT_URI: &str =
    "https://stable.dev.lcip.org/oauth/success/3c49430b43dfba77";

/// usable from StructOpt
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum FxaConfigUrl {
    StableDev,
    Stage,
    Release,
    Custom(url::Url),
}

impl FxaConfigUrl {
    pub fn default_config(&self) -> Option<Config> {
        match self {
            FxaConfigUrl::StableDev => Some(Config::stable_dev(
                STABLE_DEV_CLIENT_ID,
                STABLE_DEV_REDIRECT_URI,
            )),
            FxaConfigUrl::Release => Some(Config::release(CLIENT_ID, REDIRECT_URI)),
            _ => None,
        }
    }
    pub fn to_config(&self, client_id: &str, redirect: &str) -> Config {
        match self {
            FxaConfigUrl::StableDev => Config::stable_dev(client_id, redirect),
            FxaConfigUrl::Stage => Config::stage_dev(client_id, redirect),
            FxaConfigUrl::Release => Config::release(client_id, redirect),
            FxaConfigUrl::Custom(url) => Config::new(url.as_str(), client_id, redirect),
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

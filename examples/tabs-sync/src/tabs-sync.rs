/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![warn(rust_2018_idioms)]

use cli_support::fxa_creds::{get_account_and_token, get_cli_fxa, get_default_fxa_config};
use cli_support::prompt::prompt_char;
use std::path::Path;
use std::sync::Arc;
use structopt::StructOpt;
use tabs::{RemoteTabRecord, TabsStore};

use anyhow::Result;

#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "tabs_sync", about = "CLI for Sync tabs store")]
pub struct Opts {
    #[structopt(
        name = "credential_file",
        value_name = "CREDENTIAL_JSON",
        long = "credentials",
        short = "c",
        default_value = "./credentials.json"
    )]
    /// Path to credentials.json.
    pub creds_file: String,

    #[structopt(
        name = "database_path",
        value_name = "DATABASE_PATH",
        long,
        short = "d",
        default_value = "./tab-sync.db"
    )]
    /// Path to the database, which will only be created after a sync with incoming records.
    pub db_path: String,
}

fn ms_to_string(ms: i64) -> String {
    use chrono::{DateTime, Local};
    use std::time::{Duration, UNIX_EPOCH};
    let time = UNIX_EPOCH + Duration::from_millis(ms as u64);
    let dtl: DateTime<Local> = time.into();
    dtl.format("%F %r").to_string()
}

fn main() -> Result<()> {
    viaduct_reqwest::use_reqwest_backend();
    cli_support::init_logging();
    let opts = Opts::from_args();

    let (_, token_info) = get_account_and_token(get_default_fxa_config(), &opts.creds_file)?;
    let sync_key = base64::encode_config(
        &token_info.key.unwrap().key_bytes()?,
        base64::URL_SAFE_NO_PAD,
    );

    let mut cli_fxa = get_cli_fxa(get_default_fxa_config(), &opts.creds_file)?;
    let device_id = cli_fxa.account.get_current_device_id()?;

    let store = Arc::new(TabsStore::new(Path::new(&opts.db_path)));

    loop {
        match prompt_char("[U]pdate local state, [L]ist remote tabs, [S]ync or [Q]uit")
            .unwrap_or('?')
        {
            'U' | 'u' => {
                log::info!("Updating the local state.");
                let local_state = read_local_state();
                dbg!(&local_state);
                store
                    .storage
                    .lock()
                    .unwrap()
                    .update_local_state(local_state);
            }
            'L' | 'l' => {
                log::info!("Listing remote tabs.");
                let tabs_and_clients = match store.remote_tabs() {
                    Some(tc) => tc,
                    None => {
                        println!("No remote tabs! Did you try syncing first?");
                        continue;
                    }
                };
                println!("--------------------------------");
                for tabs_and_client in tabs_and_clients {
                    let modified = ms_to_string(tabs_and_client.last_modified);
                    println!(
                        "> {} ({}) - {}",
                        tabs_and_client.client_id, tabs_and_client.client_name, modified
                    );
                    for tab in tabs_and_client.remote_tabs {
                        let (first, rest) = tab.url_history.split_first().unwrap();
                        println!(
                            "  - {} ({}, {})",
                            tab.title,
                            first,
                            ms_to_string(tab.last_used)
                        );
                        for url in rest {
                            println!("      {}", url);
                        }
                    }
                }
                println!("--------------------------------");
            }
            'S' | 's' => {
                log::info!("Syncing!");
                match Arc::clone(&store).sync(
                    cli_fxa.client_init.clone().key_id,
                    cli_fxa.client_init.clone().access_token,
                    sync_key.clone(),
                    cli_fxa.client_init.tokenserver_url.to_string(),
                    device_id.clone(),
                ) {
                    Err(e) => {
                        log::warn!("Sync failed! {}", e);
                    }
                    Ok(sync_ping) => {
                        log::info!("Sync was successful!");
                        log::info!(
                            "Sync telemetry: {}",
                            serde_json::to_string_pretty(&sync_ping).unwrap()
                        );
                    }
                }
            }
            'Q' | 'q' => {
                break;
            }
            '?' => {
                continue;
            }
            c => {
                println!("Unknown action '{}', exiting.", c);
                break;
            }
        }
    }
    Ok(())
}

#[cfg(feature = "with-clipboard")]
fn read_local_state() -> Vec<RemoteTabRecord> {
    use clipboard::{ClipboardContext, ClipboardProvider};
    println!("Please run the following command in the Firefox Browser Toolbox and copy it.");
    println!(
        "   JSON.stringify(await Weave.Service.engineManager.get(\"tabs\")._store.getAllTabs())"
    );
    println!("Because of platform limitations, we can't let you paste a long string here.");
    println!("So instead we'll read from your clipboard. Press ENTER when ready!");

    prompt_char("Ready?").unwrap_or_default();

    let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
    let json = ctx.get_contents().unwrap();

    // Yeah we double parse coz the devtools console wraps the result in quotes. Sorry.
    let json: serde_json::Value = serde_json::from_str(&json).unwrap();
    let json: serde_json::Value = serde_json::from_str(json.as_str().unwrap()).unwrap();

    let tabs = json.as_array().unwrap();

    let mut local_state = vec![];
    for tab in tabs {
        let title = tab["title"].as_str().unwrap().to_owned();
        let last_used = tab["lastUsed"].as_i64().unwrap();
        let icon = tab["icon"]
            .as_str()
            .map(|s| Some(s.to_owned()))
            .unwrap_or(None);
        let url_history = tab["urlHistory"].as_array().unwrap();
        let url_history = url_history
            .iter()
            .map(|u| u.as_str().unwrap().to_owned())
            .collect();
        local_state.push(RemoteTabRecord {
            title,
            url_history,
            icon,
            last_used,
        });
    }
    local_state
}

#[cfg(not(feature = "with-clipboard"))]
fn read_local_state() -> Vec<RemoteTabRecord> {
    println!("This module is build without the `clipboard` feature, so we can't");
    println!("read the local state.");
    println!("Instead, we'll write one dummy tab:");
    vec![RemoteTabRecord {
        title: "Mozilla".to_string(),
        url_history: vec!["https://www.mozilla.org".to_string()],
        icon: None,
        last_used: 0,
    }]
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![warn(rust_2018_idioms)]

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use cli_support::fxa_creds::{
    get_account_and_token, get_cli_fxa, get_default_fxa_config, SYNC_SCOPE,
};
use cli_support::prompt::{prompt_char, prompt_string};
use interrupt_support::NeverInterrupts;
use std::path::Path;
use std::sync::Arc;
use structopt::StructOpt;
use sync15::{
    client::{sync_multiple, MemoryCachedState, Sync15StorageClientInit},
    KeyBundle,
};
use tabs::{RemoteTabRecord, TabsEngine, TabsStore};

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

fn do_sync(
    store: Arc<TabsStore>,
    key_id: String,
    access_token: String,
    sync_key: String,
    tokenserver_url: url::Url,
    local_id: String,
) -> Result<String> {
    let mut mem_cached_state = MemoryCachedState::default();
    let engine = TabsEngine::new(Arc::clone(&store));

    // Since we are syncing without the sync manager, there's no
    // command processor, therefore no clients engine, and in
    // consequence `TabsStore::prepare_for_sync` is never called
    // which means our `local_id` will never be set.
    // Do it here.
    *engine.local_id.write().unwrap() = local_id;

    let storage_init = &Sync15StorageClientInit {
        key_id,
        access_token,
        tokenserver_url: url::Url::parse(tokenserver_url.as_str())?,
    };
    let root_sync_key = &KeyBundle::from_ksync_base64(sync_key.as_str())?;

    let mut result = sync_multiple(
        &[&engine],
        &mut None,
        &mut mem_cached_state,
        storage_init,
        root_sync_key,
        &NeverInterrupts,
        None,
    );

    if let Err(e) = result.result {
        return Err(e.into());
    }
    match result.engine_results.remove("tabs") {
        None | Some(Ok(())) => Ok(serde_json::to_string(&result.telemetry)?),
        Some(Err(e)) => Err(e.into()),
    }
}

fn main() -> Result<()> {
    viaduct_reqwest::use_reqwest_backend();
    cli_support::init_logging();
    let opts = Opts::from_args();

    let (_, token_info) =
        get_account_and_token(get_default_fxa_config(), &opts.creds_file, &[SYNC_SCOPE])?;
    let sync_key = URL_SAFE_NO_PAD.encode(token_info.key.unwrap().key_bytes()?);

    let cli_fxa = get_cli_fxa(get_default_fxa_config(), &opts.creds_file, &[SYNC_SCOPE])?;
    let device_id = cli_fxa.account.get_current_device_id()?;

    let store = Arc::new(TabsStore::new(Path::new(&opts.db_path)));

    loop {
        match prompt_char(
            "[U]pdate local state, Update with a [d]ummy tab, [L]ist remote tabs, [S]ync or [Q]uit",
        )
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
            'D' | 'd' => {
                log::info!("Updating the local state with a dummy mozilla.org tab.");
                let tabs = vec![RemoteTabRecord {
                    title: "Mozilla".to_string(),
                    url_history: vec!["https://www.mozilla.org".to_string()],
                    ..Default::default()
                }];
                dbg!(&tabs);
                store.storage.lock().unwrap().update_local_state(tabs);
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
                match do_sync(
                    Arc::clone(&store),
                    cli_fxa.client_init.clone().key_id,
                    cli_fxa.client_init.clone().access_token,
                    sync_key.clone(),
                    cli_fxa.client_init.tokenserver_url.clone(),
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

fn read_local_state() -> Vec<RemoteTabRecord> {
    println!("Please run the following command in the Firefox Browser Toolbox:");
    println!(
        "   JSON.stringify(await Weave.Service.engineManager.get(\"tabs\").getTabsWithinPayloadSize())"
    );
    println!("And paste the contents into a file. Then enter the name of that file:");

    let filename = prompt_string("Filename").unwrap_or_default();
    let json = std::fs::read_to_string(filename).expect("Failed to read from the file");

    // Devtools writes the output in single-quotes, which we want to trim. If also might cause
    // trailing whitespace and trailing zero-width-space (`u{200b}`) (which isn't considered
    // whitespace!?)
    // So trim all those things...
    let json = json.trim_matches(|c: char| c.is_whitespace() || c == '\'' || c == '\u{200b}');
    let json: serde_json::Value = serde_json::from_str(json).unwrap();

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
            inactive: false,
        });
    }
    local_state
}

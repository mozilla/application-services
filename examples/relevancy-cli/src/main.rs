/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::sync::Arc;

use clap::Parser;
use cli_support::{
    fxa_creds::{get_cli_fxa, get_default_fxa_config, SYNC_SCOPE},
    remote_settings_service,
};
use env_logger::Builder;
use interrupt_support::NeverInterrupts;
use places::{ConnectionType, PlacesApi};
use relevancy::RelevancyStore;
use sync15::client::{sync_multiple, MemoryCachedState};
use sync15::engine::SyncEngineId;

use anyhow::{bail, Result};

static CREDENTIALS_PATH: &str = ".cli-data/credentials.json";

#[derive(Parser)]
#[command(about, long_about = None)]
struct Cli {
    /// Printout extra details, including how each URL is classified
    #[clap(long, short, action)]
    verbose: bool,

    /// Load places data from disk rather than syncing it
    ///
    /// Note: this only works for mobile places databases, desktop uses a different format.
    #[clap(long)]
    places_db: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    nss::ensure_initialized();
    viaduct_reqwest::use_reqwest_backend();
    if let Some(dir) = std::path::PathBuf::from(CREDENTIALS_PATH).parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut builder = Builder::new();
    builder.filter_level(log::LevelFilter::Info);
    if cli.verbose {
        builder.filter_module("relevancy", log::LevelFilter::Trace);
    }
    builder.init();
    println!("================== Initializing Relevancy ===================");
    let relevancy_store = RelevancyStore::new(
        "file:relevancy-cli-relevancy?mode=memory&cache=shared".to_owned(),
        remote_settings_service(),
    );
    relevancy_store.ensure_interest_data_populated()?;

    println!("==================== Downloading History ====================");
    let places_api = if let Some(path) = cli.places_db {
        PlacesApi::new(path)?
    } else {
        let places_api = PlacesApi::new_memory("relevancy-cli-places")?;
        sync_places(&places_api)?;
        places_api
    };

    let conn = places_api.open_connection(ConnectionType::ReadOnly)?;
    let top_frecency_info = places::storage::history::get_top_frecent_site_infos(&conn, 5000, 0)?;
    let top_frecency_urls = top_frecency_info
        .into_iter()
        .map(|info| info.url.to_string())
        .collect();
    println!("==================== Calculated Interests====================");
    let interest_vector = relevancy_store.ingest(top_frecency_urls)?;
    interest_vector.print_all_counts();

    Ok(())
}

fn sync_places(places_api: &Arc<PlacesApi>) -> Result<()> {
    Arc::clone(places_api).register_with_sync_manager();
    let cli_fxa = get_cli_fxa(get_default_fxa_config(), CREDENTIALS_PATH, &[SYNC_SCOPE])?;
    let mut mem_cached_state = MemoryCachedState::default();
    let mut global_state: Option<String> = None;
    let engine = places::get_registered_sync_engine(&SyncEngineId::History)
        .expect("no registered sync engine");
    let result = sync_multiple(
        &[&*engine],
        &mut global_state,
        &mut mem_cached_state,
        &cli_fxa.client_init.clone(),
        &cli_fxa.as_key_bundle()?,
        &NeverInterrupts,
        None,
    );

    if result.engine_results.len() != 1 {
        bail!(
            "Unexpected number of engine result: {}",
            result.engine_results.len()
        );
    }

    match result.result {
        Ok(()) => log::info!("Sync successful"),
        Err(e) => {
            log::info!("Sync failed");
            bail!(e);
        }
    }
    Ok(())
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![warn(rust_2018_idioms)]

use cli_support::fxa_creds::{get_cli_fxa, get_default_fxa_config};
use places::PlacesApi;
use serde_derive::*;
use std::convert::TryFrom;
use structopt::StructOpt;
use sync15::{
    sync_multiple, EngineSyncAssociation, MemoryCachedState, SetupStorageClient,
    Sync15StorageClient, SyncEngine, SyncEngineId,
};
use sync_guid::Guid as SyncGuid;
use url::Url;
use viaduct_reqwest::use_reqwest_backend;

use anyhow::Result;

// A struct in the format of desktop with a union of all fields.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct DesktopItem {
    type_code: u8,
    guid: Option<SyncGuid>,
    date_added: Option<u64>,
    last_modified: Option<u64>,
    title: Option<String>,
    uri: Option<Url>,
    children: Vec<DesktopItem>,
}

#[allow(clippy::too_many_arguments)]
fn sync(
    mut engine_names: Vec<String>,
    cred_file: String,
    wipe_all: bool,
    wipe: bool,
    reset: bool,
    nsyncs: u32,
    wait: u64,
) -> Result<()> {
    use_reqwest_backend();

    let cli_fxa = get_cli_fxa(get_default_fxa_config(), &cred_file)?;

    if wipe_all {
        Sync15StorageClient::new(cli_fxa.client_init.clone())?.wipe_all_remote()?;
    }
    // phew - working with traits is making markh's brain melt!
    // Note also that PlacesApi::sync() exists and ultimately we should
    // probably end up using that, but it's not yet ready to handle bookmarks.
    // And until we move to PlacesApi::sync() we simply do not persist any
    // global state at all (however, we do reuse the in-memory state).
    let mut mem_cached_state = MemoryCachedState::default();
    let mut global_state: Option<String> = None;
    let engines: Vec<Box<dyn SyncEngine>> = if engine_names.is_empty() {
        vec![
            places::get_registered_sync_engine(&SyncEngineId::Bookmarks).unwrap(),
            places::get_registered_sync_engine(&SyncEngineId::History).unwrap(),
        ]
    } else {
        engine_names.sort();
        engine_names.dedup();
        engine_names
            .into_iter()
            .map(|name| {
                places::get_registered_sync_engine(&SyncEngineId::try_from(name.as_ref()).unwrap())
                    .unwrap()
            })
            .collect()
    };
    for engine in &engines {
        if wipe {
            engine.wipe()?;
        }
        if reset {
            engine.reset(&EngineSyncAssociation::Disconnected)?;
        }
    }

    // now the syncs.
    // For now we never persist the global state, which means we may lose
    // which engines are declined.
    // That's OK for the short term, and ultimately, syncing functionality
    // will be in places_api, which will give us this for free.

    let mut error_to_report = None;
    let engines_to_sync: Vec<&dyn SyncEngine> = engines.iter().map(AsRef::as_ref).collect();

    for n in 0..nsyncs {
        let mut result = sync_multiple(
            &engines_to_sync,
            &mut global_state,
            &mut mem_cached_state,
            &cli_fxa.client_init.clone(),
            &cli_fxa.root_sync_key,
            &interrupt_support::NeverInterrupts,
            None,
        );

        for (name, result) in result.engine_results.drain() {
            match result {
                Ok(()) => log::info!("Status for {:?}: Ok", name),
                Err(e) => {
                    log::warn!("Status for {:?}: {:?}", name, e);
                    error_to_report = Some(e);
                }
            }
        }

        match result.result {
            Err(e) => {
                log::warn!("Sync failed! {}", e);
                log::warn!("BT: {:?}", e.backtrace());
                error_to_report = Some(e);
            }
            Ok(()) => log::info!("Sync was successful!"),
        }

        println!("Sync service status: {:?}", result.service_status);
        println!(
            "Sync telemetry: {}",
            serde_json::to_string_pretty(&result.telemetry).unwrap()
        );

        if n < nsyncs - 1 {
            log::info!("Waiting {}ms before syncing again...", wait);
            std::thread::sleep(std::time::Duration::from_millis(wait));
        }
    }

    // return an error if any engine failed.
    match error_to_report {
        Some(e) => Err(e.into()),
        None => Ok(()),
    }
}

// Note: this uses doc comments to generate the help text.
#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "places-utils", about = "Command-line utilities for places")]
pub struct Opts {
    #[structopt(
        name = "database_path",
        long,
        short = "d",
        default_value = "./places.db"
    )]
    /// Path to the database, which will be created if it doesn't exist.
    pub database_path: String,

    /// Leaves all logging disabled, which may be useful when evaluating perf
    #[structopt(name = "no-logging", long)]
    pub no_logging: bool,

    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Clone, Debug, StructOpt)]
enum Command {
    #[structopt(name = "sync")]
    /// Syncs all or some engines.
    Sync {
        #[structopt(name = "engines", long)]
        /// The names of the engines to sync. If not specified, all engines
        /// will be synced.
        engines: Vec<String>,

        /// Path to store our cached fxa credentials.
        #[structopt(name = "credentials", long, default_value = "./credentials.json")]
        credential_file: String,

        /// Wipe ALL storage from the server before syncing.
        #[structopt(name = "wipe-all-remote", long)]
        wipe_all: bool,

        /// Wipe the engine data from the server before syncing.
        #[structopt(name = "wipe-remote", long)]
        wipe: bool,

        /// Reset the engine before syncing
        #[structopt(name = "reset", long)]
        reset: bool,

        /// Number of syncs to perform
        #[structopt(name = "nsyncs", long, default_value = "1")]
        nsyncs: u32,

        /// Number of milliseconds to wait between syncs
        #[structopt(name = "wait", long, default_value = "0")]
        wait: u64,
    },
}

fn main() -> Result<()> {
    let opts = Opts::from_args();
    if !opts.no_logging {
        cli_support::init_trace_logging();
    }

    let db_path = opts.database_path;
    let api = PlacesApi::new(&db_path)?;
    // Needed to make the get_registered_sync_engine() calls work.
    api.clone().register_with_sync_manager();
    ctrlc::set_handler(move || {
        if !shutdown::in_shutdown() {
            println!("\nCTRL-C detected, enabling shutdown mode\n");
            shutdown::shutdown();
        } else {
            println!("\nCTRL-C detected disabling shutdown mode\n");
            shutdown::restart();
        }
    })
    .unwrap();

    match opts.cmd {
        Command::Sync {
            engines,
            credential_file,
            wipe_all,
            wipe,
            reset,
            nsyncs,
            wait,
        } => sync(
            engines,
            credential_file,
            wipe_all,
            wipe,
            reset,
            nsyncs,
            wait,
        ),
    }
}

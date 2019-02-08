/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use cli_support::fxa_creds::{get_cli_fxa, get_default_fxa_config};
use failure::Fail;
use places::history_sync::store::HistoryStore;
use places::PlacesDb;
use serde_json;
use sync15::{telemetry, SetupStorageClient, Store, Sync15StorageClient};

// I'm completely punting on good error handling here.
type Result<T> = std::result::Result<T, failure::Error>;

fn init_logging() {
    // Explicitly ignore some rather noisy crates. Turn on trace for everyone else.
    let spec = "trace,tokio_threadpool=warn,tokio_reactor=warn,tokio_core=warn,tokio=warn,hyper=warn,want=warn,mio=warn,reqwest=warn";
    env_logger::init_from_env(env_logger::Env::default().filter_or("RUST_LOG", spec));
}

fn main() -> Result<()> {
    let matches = clap::App::new("sync_history")
        .about("History syncing tool")
        .arg(
            clap::Arg::with_name("database_path")
                .short("d")
                .long("database")
                .value_name("PLACES_DATABASE")
                .takes_value(true)
                .help("Path to the places database (default: \"./places.db\")"),
        )
        .arg(
            clap::Arg::with_name("encryption_key")
                .short("k")
                .long("key")
                .value_name("ENCRYPTION_KEY")
                .takes_value(true)
                .help("Database encryption key."),
        )
        .arg(
            clap::Arg::with_name("credential_file")
                .short("c")
                .long("credentials")
                .value_name("CREDENTIAL_JSON")
                .takes_value(true)
                .help(
                    "Path to store our cached fxa credentials (defaults to \"./credentials.json\"",
                ),
        )
        .arg(
            clap::Arg::with_name("no_logging")
                .short("n")
                .long("no-logging")
                .value_name("SKIP_LOGGING")
                .takes_value(false)
                .help("Disables all logging, which may be useful when evaluating perf"),
        )
        .arg(
            clap::Arg::with_name("reset")
                .short("r")
                .long("reset")
                .help("Reset the store before syncing"),
        )
        .arg(
            clap::Arg::with_name("wipe-remote")
                .short("w")
                .long("wipe-remote")
                .help("Wipe the server store before syncing"),
        )
        .get_matches();

    if !matches.is_present("no_logging") {
        init_logging();
    }

    let cred_file = matches
        .value_of("credential_file")
        .unwrap_or("./credentials.json");
    let db_path = matches.value_of("database_path").unwrap_or("./places.db");
    // This should already be checked by `clap`, IIUC
    let encryption_key = matches.value_of("encryption_key");

    // Lets not log the encryption key, it's just not a good habit to be in.
    log::debug!(
        "Using credential file = {:?}, db = {:?}",
        cred_file,
        db_path
    );

    // TODO: allow users to use stage/etc.
    let cli_fxa = get_cli_fxa(get_default_fxa_config(), cred_file)?;

    let db = PlacesDb::open(db_path, encryption_key)?;
    let store = HistoryStore::new(&db);

    if matches.is_present("wipe-remote") {
        log::info!("Wiping remote");
        let client = Sync15StorageClient::new(cli_fxa.client_init.clone())?;
        client.wipe_all_remote()?;
    }

    if matches.is_present("reset") {
        log::info!("Resetting");
        store.reset()?;
    }

    log::info!("Syncing!");
    let mut sync_ping = telemetry::SyncTelemetryPing::new();
    if let Err(e) = store.sync(&cli_fxa.client_init, &cli_fxa.root_sync_key, &mut sync_ping) {
        log::warn!("Sync failed! {}", e);
        log::warn!("BT: {:?}", e.backtrace());
    } else {
        log::info!("Sync was successful!");
    }
    println!(
        "Sync telemetry: {}",
        serde_json::to_string_pretty(&sync_ping).unwrap()
    );
    println!("Exiting (bye!)");
    Ok(())
}

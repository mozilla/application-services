/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use failure::Fail;
use fxa_client::{AccessTokenInfo, Config, FirefoxAccount};
use places::history_sync::store::HistoryStore;
use places::PlacesDb;
use std::{fs, io::Read};
use sync15::client::SetupStorageClient;
use sync15::{KeyBundle, Store, Sync15StorageClient, Sync15StorageClientInit, telemetry};
use serde_json;


const CLIENT_ID: &str = "";
const REDIRECT_URI: &str = "";
const SYNC_SCOPE: &str = "https://identity.mozilla.com/apps/oldsync";

// I'm completely punting on good error handling here.
type Result<T> = std::result::Result<T, failure::Error>;

fn load_fxa_creds(path: &str) -> Result<FirefoxAccount> {
    let mut file = fs::File::open(path)?;
    let mut s = String::new();
    file.read_to_string(&mut s)?;
    Ok(FirefoxAccount::from_json(&s)?)
}

fn init_logging() {
    // Explicitly ignore some rather noisy crates. Turn on trace for everyone else.
    let spec = "trace,tokio_threadpool=warn,tokio_reactor=warn,tokio_core=warn,tokio=warn,hyper=warn,want=warn,mio=warn,reqwest=warn";
    env_logger::init_from_env(env_logger::Env::default().filter_or("RUST_LOG", spec));
}

fn main() -> Result<()> {
    init_logging();

    let matches = clap::App::new("sync_history")
        .about("History syncing tool")
        .arg(
            clap::Arg::with_name("database_path")
                .short("d")
                .long("database")
                .value_name("LOGINS_DATABASE")
                .takes_value(true)
                .help("Path to the logins database (default: \"./logins.db\")"),
        )
        .arg(
            clap::Arg::with_name("encryption_key")
                .short("k")
                .long("key")
                .value_name("ENCRYPTION_KEY")
                .takes_value(true)
                .help("Database encryption key.")
                .required(true),
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

    let cred_file = matches
        .value_of("credential_file")
        .unwrap_or("./credentials.json");
    let db_path = matches.value_of("database_path").unwrap_or("./logins.db");
    // This should already be checked by `clap`, IIUC
    let encryption_key = matches
        .value_of("encryption_key")
        .expect("Encryption key is not optional");

    // Lets not log the encryption key, it's just not a good habit to be in.
    log::debug!(
        "Using credential file = {:?}, db = {:?}",
        cred_file,
        db_path
    );

    // TODO: allow users to use stage/etc.
    let cfg = Config::release(CLIENT_ID, REDIRECT_URI);
    let tokenserver_url = cfg.token_server_endpoint_url()?;

    // TODO: we should probably set a persist callback on acct?
    let mut acct = load_fxa_creds(cred_file)?;
    let token_info: AccessTokenInfo = match acct.get_access_token(SYNC_SCOPE) {
        Ok(t) => t,
        Err(_) => {
            panic!("No creds - run some other tool to set them up.");
        }
    };
    let key = token_info.key.unwrap();

    let client_init = Sync15StorageClientInit {
        key_id: key.kid.clone(),
        access_token: token_info.token.clone(),
        tokenserver_url,
    };
    let root_sync_key = KeyBundle::from_ksync_bytes(&key.key_bytes()?)?;

    let db = PlacesDb::open(db_path, Some(encryption_key))?;
    let store = HistoryStore::new(&db);

    if matches.is_present("wipe-remote") {
        log::info!("Wiping remote");
        let client = Sync15StorageClient::new(client_init.clone())?;
        client.wipe_all_remote()?;
    }

    if matches.is_present("reset") {
        log::info!("Resetting");
        store.reset()?;
    }

    log::info!("Syncing!");
    let mut telem_sync = telemetry::Sync::new();
    if let Err(e) = store.sync(&client_init, &root_sync_key, &mut telem_sync) {
        log::warn!("Sync failed! {}", e);
        log::warn!("BT: {:?}", e.backtrace());
    } else {
        log::info!("Sync was successful!");
    }
    telem_sync.finished();
    println!("telemetry: {:?}", serde_json::to_string(&telem_sync).unwrap());
    println!("Exiting (bye!)");
    Ok(())
}

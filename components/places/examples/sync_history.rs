/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate sync15_adapter;
extern crate fxa_client;
extern crate url;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

extern crate rusqlite;

extern crate clap;

#[macro_use]
extern crate log;
extern crate env_logger;
extern crate failure;

extern crate places;

use failure::Fail;

use std::{fs, io::{Read}};
use fxa_client::{FirefoxAccount, Config, AccessTokenInfo};
use sync15_adapter::{Sync15StorageClient, Sync15StorageClientInit, KeyBundle, Store};
use sync15_adapter::client::SetupStorageClient;
use places::{PlacesDb};
use places::history_sync::store::{HistoryStore};

const CONTENT_BASE: &str = "https://accounts.firefox.com";
const SYNC_SCOPE: &str = "https://identity.mozilla.com/apps/oldsync";

// I'm completely punting on good error handling here.
type Result<T> = std::result::Result<T, failure::Error>;

#[derive(Debug, Deserialize)]
struct ScopedKeyData {
    k: String,
    kty: String,
    kid: String,
    scope: String,
}

fn load_fxa_creds(path: &str) -> Result<FirefoxAccount> {
    let mut file = fs::File::open(path)?;
    let mut s = String::new();
    file.read_to_string(&mut s)?;
    Ok(FirefoxAccount::from_json(&s)?)
}


fn init_logging() {
    // Explicitly ignore some rather noisy crates. Turn on trace for everyone else.
    let spec = "trace,tokio_threadpool=warn,tokio_reactor=warn,tokio_core=warn,tokio=warn,hyper=warn,want=warn,mio=warn,reqwest=warn";
    env_logger::init_from_env(
        env_logger::Env::default().filter_or("RUST_LOG", spec)
    );
}

fn main() -> Result<()> {
    init_logging();

    let matches = clap::App::new("sync_history")
        .about("History syncing tool")

        .arg(clap::Arg::with_name("database_path")
            .short("d")
            .long("database")
            .value_name("LOGINS_DATABASE")
            .takes_value(true)
            .help("Path to the logins database (default: \"./logins.db\")"))

        .arg(clap::Arg::with_name("encryption_key")
            .short("k")
            .long("key")
            .value_name("ENCRYPTION_KEY")
            .takes_value(true)
            .help("Database encryption key.")
            .required(true))

        .arg(clap::Arg::with_name("credential_file")
            .short("c")
            .long("credentials")
            .value_name("CREDENTIAL_JSON")
            .takes_value(true)
            .help("Path to store our cached fxa credentials (defaults to \"./credentials.json\""))

        .arg(clap::Arg::with_name("reset")
            .short("r")
            .long("reset")
            .help("Reset the store before syncing"))

        .arg(clap::Arg::with_name("wipe-remote")
            .short("w")
            .long("wipe-remote")
            .help("Wipe the server store before syncing"))

        .get_matches();

    let cred_file = matches.value_of("credential_file").unwrap_or("./credentials.json");
    let db_path = matches.value_of("database_path").unwrap_or("./logins.db");
    // This should already be checked by `clap`, IIUC
    let encryption_key = matches.value_of("encryption_key").expect("Encryption key is not optional");

    // Lets not log the encryption key, it's just not a good habit to be in.
    debug!("Using credential file = {:?}, db = {:?}", cred_file, db_path);

    // TODO: allow users to use stage/etc.
    let cfg = Config::import_from(CONTENT_BASE)?;
    let tokenserver_url = cfg.token_server_endpoint_url()?;

    // TODO: we should probably set a persist callback on acct?
    let mut acct = load_fxa_creds(cred_file)?;
    let token_info: AccessTokenInfo = match acct.get_access_token(SYNC_SCOPE) {
        Ok(t) => t,
        Err(_) => {
            panic!("No creds - run some other tool to set them up.");
        }
    };

    let key: ScopedKeyData = serde_json::from_str(&token_info.key.unwrap())?;

    let client_init = Sync15StorageClientInit {
        key_id: key.kid.clone(),
        access_token: token_info.token.clone(),
        tokenserver_url,
    };
    let root_sync_key = KeyBundle::from_ksync_base64(&key.k)?;

    let db = PlacesDb::open(db_path, Some(encryption_key))?;
    let store = HistoryStore::new(&db);

    if matches.is_present("wipe-remote") {
        info!("Wiping remote");
        let client = Sync15StorageClient::new(client_init.clone())?;
        client.wipe_all_remote()?;
    }

    if matches.is_present("reset") {
        info!("Resetting");
        store.reset()?;
    }

    info!("Syncing!");
    if let Err(e) = store.sync(&client_init, &root_sync_key) {
        warn!("Sync failed! {}", e);
        warn!("BT: {:?}", e.backtrace());
    } else {
        info!("Sync was successful!");
    }
    println!("Exiting (bye!)");
    Ok(())
}

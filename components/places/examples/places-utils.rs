/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use cli_support::fxa_creds::{get_cli_fxa, get_default_fxa_config};
use places::history_sync::store::HistoryStore;
use places::storage::bookmarks::{
    fetch_tree, insert_tree, BookmarkNode, BookmarkRootGuid, BookmarkTreeNode, FolderNode,
    SeparatorNode,
};
use places::types::{BookmarkType, SyncGuid, Timestamp};
use places::PlacesDb;

use serde_derive::*;
use sql_support::ConnExt;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use structopt::StructOpt;
use url::Url;
use sync15::{telemetry, Store};
use failure::Fail;

type Result<T> = std::result::Result<T, failure::Error>;

fn init_logging() {
    // Explicitly ignore some rather noisy crates. Turn on trace for everyone else.
    let spec = "trace,tokio_threadpool=warn,tokio_reactor=warn,tokio_core=warn,tokio=warn,hyper=warn,want=warn,mio=warn,reqwest=warn";
    env_logger::init_from_env(env_logger::Env::default().filter_or("RUST_LOG", spec));
}

// A struct in the format of desktop with a union of all fields.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct DesktopItem {
    type_code: u8,
    guid: Option<SyncGuid>,
    date_added: Option<u64>,
    last_modified: Option<u64>,
    title: Option<String>,
    #[serde(with = "url_serde")]
    uri: Option<Url>,
    children: Vec<DesktopItem>,
}

fn convert_node(dm: DesktopItem) -> Option<BookmarkTreeNode> {
    let bookmark_type = BookmarkType::from_u8_with_valid_url(dm.type_code, || dm.uri.is_some());

    Some(match bookmark_type {
        BookmarkType::Bookmark => {
            let url = match dm.uri {
                Some(uri) => uri,
                None => {
                    log::warn!("ignoring bookmark node without url: {:?}", dm);
                    return None;
                }
            };
            BookmarkNode {
                guid: dm.guid,
                date_added: dm.date_added.map(|v| Timestamp(v / 1000)),
                last_modified: dm.last_modified.map(|v| Timestamp(v / 1000)),
                title: dm.title,
                url,
            }
            .into()
        }
        BookmarkType::Separator => SeparatorNode {
            guid: dm.guid,
            date_added: dm.date_added.map(|v| Timestamp(v / 1000)),
            last_modified: dm.last_modified.map(|v| Timestamp(v / 1000)),
        }
        .into(),
        BookmarkType::Folder => FolderNode {
            guid: dm.guid,
            date_added: dm.date_added.map(|v| Timestamp(v / 1000)),
            last_modified: dm.last_modified.map(|v| Timestamp(v / 1000)),
            title: dm.title,
            children: dm
                .children
                .into_iter()
                .filter_map(|c| convert_node(c))
                .collect(),
        }
        .into(),
    })
}

fn do_import(db: &PlacesDb, root: BookmarkTreeNode) -> Result<()> {
    // We need to import each of the sub-trees individually.
    // Later we will want to get smarter around guids - currently we will
    // fail to do this twice due to guid dupes - but that's OK for now.
    let folder = match root {
        BookmarkTreeNode::Folder(folder_node) => folder_node,
        _ => {
            println!("Imported node isn't a folder structure");
            return Ok(());
        }
    };
    let is_root = match folder.guid {
        Some(ref guid) => BookmarkRootGuid::Root == *guid,
        None => false,
    };
    if !is_root {
        // later we could try and import a sub-tree.
        println!("Imported tree isn't the root node");
        return Ok(());
    }

    for sub_root_node in folder.children {
        let sub_root_folder = match sub_root_node {
            BookmarkTreeNode::Folder(folder_node) => folder_node,
            _ => {
                println!("Child of the root isn't a folder - skipping...");
                continue;
            }
        };
        println!("importing {:?}", sub_root_folder.guid);
        insert_tree(db, &sub_root_folder)?
    }
    Ok(())
}

fn run_desktop_import(db: &PlacesDb, filename: String) -> Result<()> {
    println!("import from {}", filename);

    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    let m: DesktopItem = serde_json::from_reader(reader)?;
    // convert mapping into our tree.
    let root = match convert_node(m) {
        Some(node) => node,
        None => {
            println!("Failed to read a tree from this file");
            return Ok(());
        }
    };
    do_import(db, root)
}

fn run_native_import(db: &PlacesDb, filename: String) -> Result<()> {
    println!("import from {}", filename);

    let file = File::open(filename)?;
    let reader = BufReader::new(file);

    let root: BookmarkTreeNode = serde_json::from_reader(reader)?;
    do_import(db, root)
}

fn run_native_export(db: &PlacesDb, filename: String) -> Result<()> {
    println!("export to {}", filename);

    let file = File::create(filename)?;
    let writer = BufWriter::new(file);

    let tree = fetch_tree(db.conn(), &BookmarkRootGuid::Root.into())?.unwrap();
    serde_json::to_writer_pretty(writer, &tree)?;
    Ok(())
}

fn sync(db: &PlacesDb, engine_names: Vec<String>, cred_file: String, wipe: bool, reset: bool) -> Result<()> {
    let cli_fxa = get_cli_fxa(get_default_fxa_config(), &cred_file)?;

    // We will want this as a Vec<sync15::Store> eventually...
    let stores = if engine_names.len() == 0 {
        vec![HistoryStore::new(db)]
    } else {
        assert!(engine_names.len() == 1 && engine_names[0] == "history");
        vec![HistoryStore::new(db)]
    };
    let mut sync_ping = telemetry::SyncTelemetryPing::new();
    for store in stores {
        if wipe {
            store.wipe()?;
        }
        if reset {
            store.reset()?;
        }

        log::info!("Syncing {}", store.collection_name());
        if let Err(e) = store.sync(&cli_fxa.client_init.clone(), &cli_fxa.root_sync_key, &mut sync_ping) {
            log::warn!("Sync failed! {}", e);
            log::warn!("BT: {:?}", e.backtrace());
        } else {
            log::info!("Sync was successful!");
        }
    }
    println!(
        "Sync telemetry: {}",
        serde_json::to_string_pretty(&sync_ping).unwrap()
    );
    Ok(())
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

    #[structopt(name = "encryption_key", long, short = "k")]
    /// The database encryption key. If not specified the database will not
    /// be encrypted.
    pub encryption_key: Option<String>,

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

        /// Wipe the server store before syncing.
        #[structopt(name = "wipe-remote", long)]
        wipe: bool,

        /// Reset the store before syncing
        #[structopt(name = "reset", long)]
        reset: bool,
    },

    #[structopt(name = "export-bookmarks")]
    /// Exports bookmarks (but not in a way Desktop can import it!)
    ExportBookmarks {
        #[structopt(name = "output-file", long, short = "o")]
        /// The name of the output file where the json will be written.
        output_file: String,
    },

    #[structopt(name = "import-bookmarks")]
    /// Import bookmarks from a 'native' export (ie, as exported by this utility)
    ImportBookmarks {
        #[structopt(name = "input-file", long, short = "i")]
        /// The name of the file to read.
        input_file: String,
    },

    #[structopt(name = "import-desktop-bookmarks")]
    /// Import bookmarks from JSON file exported by desktop Firefox
    ImportDesktopBookmarks {
        #[structopt(name = "input-file", long, short = "i")]
        /// Imports bookmarks from a desktop export
        input_file: String,
    },

}

fn main() -> Result<()> {
    let opts = Opts::from_args();
    if !opts.no_logging {
        init_logging();
    }

    let db_path = opts.database_path;
    let encryption_key: Option<&str> = opts.encryption_key.as_ref().map(|s| &**s);
    let db = PlacesDb::open(db_path, encryption_key)?;

    match opts.cmd {
        Command::Sync{ engines, credential_file, wipe, reset } => sync(&db, engines, credential_file, wipe, reset),
        Command::ExportBookmarks { output_file } => run_native_export(&db, output_file),
        Command::ImportBookmarks { input_file } => run_native_import(&db, input_file),
        Command::ImportDesktopBookmarks { input_file } => run_desktop_import(&db, input_file),
    }
}

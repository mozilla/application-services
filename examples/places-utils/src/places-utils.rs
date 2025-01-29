/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![warn(rust_2018_idioms)]

use cli_support::fxa_creds::{get_cli_fxa, get_default_fxa_config, SYNC_SCOPE};
use interrupt_support::Interruptee;
use places::storage::bookmarks::{
    json_tree::{
        fetch_tree, insert_tree, BookmarkNode, BookmarkTreeNode, FetchDepth, FolderNode,
        SeparatorNode,
    },
    BookmarkRootGuid,
};
use places::types::BookmarkType;
use places::{ConnectionType, PlacesApi, PlacesDb};
use serde_derive::*;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::time::{Duration, SystemTime};
use structopt::StructOpt;
use sync15::client::{sync_multiple, MemoryCachedState, SetupStorageClient, Sync15StorageClient};
use sync15::engine::{EngineSyncAssociation, SyncEngine, SyncEngineId};
use sync_guid::Guid as SyncGuid;
use types::Timestamp;
use url::Url;
use viaduct_reqwest::use_reqwest_backend;

use anyhow::Result;

fn format_duration(d: &Duration) -> String {
    let mins = d.as_secs() / 60;
    let secs = d.as_secs() - mins * 60;
    if mins == 0 {
        format!("{secs}s")
    } else {
        format!("{mins}m {secs}s")
    }
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
            children: dm.children.into_iter().filter_map(convert_node).collect(),
        }
        .into(),
    })
}

fn do_import(db: &PlacesDb, root: BookmarkTreeNode) -> Result<()> {
    // We need to import each of the sub-trees individually.
    // Later we will want to get smarter around guids - currently we will
    // fail to do this twice due to guid dupes - but that's OK for now.
    let folder = match root {
        BookmarkTreeNode::Folder { f } => f,
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
            BookmarkTreeNode::Folder { f } => f,
            _ => {
                println!("Child of the root isn't a folder - skipping...");
                continue;
            }
        };
        println!("importing {:?}", sub_root_folder.guid);
        insert_tree(db, sub_root_folder)?
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

fn run_ios_import_history(conn: &PlacesDb, filename: String) -> Result<()> {
    let res = places::import::import_ios_history(conn, filename, 0)?;
    println!("Import finished!, results: {:?}", res);
    Ok(())
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

    let tree = fetch_tree(db, &BookmarkRootGuid::Root.into(), &FetchDepth::Deepest)?.unwrap();
    serde_json::to_writer_pretty(writer, &tree)?;
    Ok(())
}

fn run_maintenance(conn: &PlacesDb, db_size_limit: u32, count: u32) -> Result<()> {
    for _ in 0..count {
        let prune_metrics = places::storage::run_maintenance_prune(conn, db_size_limit, 6)?;
        places::storage::run_maintenance_vacuum(conn)?;
        places::storage::run_maintenance_optimize(conn)?;
        places::storage::run_maintenance_checkpoint(conn)?;
        println!("Maintenance complete");
        println!("Prune metrics: {prune_metrics:?}");
    }
    Ok(())
}

fn create_fake_visits(db: &PlacesDb, num_sites: usize, num_visits: usize) -> Result<()> {
    let tx = db.begin_transaction()?;
    let start = SystemTime::now();
    let mut this_batch = start;
    for site_num in 0..num_sites {
        let url = Url::parse(&format!("https://example{site_num}.com"))?;
        let mut st = SystemTime::now();
        for visit_num in 0..num_visits {
            let obs = places::VisitObservation::new(url.clone())
                .with_at(Some(st.into()))
                .with_visit_type(places::VisitType::Link);
            st = st.checked_sub(Duration::new(1, 0)).unwrap();
            places::storage::history::apply_observation_direct(db, obs)?;
            if interrupt_support::ShutdownInterruptee.was_interrupted() {
                println!("Interrupted");
                return Ok(());
            }
            if SystemTime::now().duration_since(this_batch)?.as_secs() > 15 {
                let total = format_duration(&SystemTime::now().duration_since(start)?);
                println!("Site number {site_num} ({visit_num} visits) - {total}...");
                this_batch = SystemTime::now();
            }
        }
    }
    places::storage::delete_pending_temp_tables(db)?;
    tx.commit()?;

    println!("Added them");
    Ok(())
}

fn delete_history(db: &PlacesDb) -> Result<()> {
    places::storage::history::delete_everything(db)?;
    Ok(())
}

fn show_stats(_db: &PlacesDb) -> Result<()> {
    println!(
        "Sorry - this has been temporarily enabled to avoid bringing our pretty-printer into m-c"
    );
    // db.execute("ANALYZE;", [])?;
    // println!("Left most column in `stat` is the record count in the table/index");
    // println!("See the sqlite docs for `sqlite_stat1` for more info.");
    // sql_support::debug_tools::print_query(db, "SELECT * from sqlite_stat1")?;
    Ok(())
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

    let cli_fxa = get_cli_fxa(get_default_fxa_config(), &cred_file, &[SYNC_SCOPE])?;

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
            &cli_fxa.as_key_bundle()?,
            &interrupt_support::ShutdownInterruptee,
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
                log::warn!("BT: {:?}", error_support::backtrace::Backtrace::new());
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

    #[structopt(name = "import-ios-history")]
    /// Import history from an iOS browser.db
    ImportIosHistory {
        #[structopt(name = "input-file", long, short = "i")]
        /// The name of the file to read
        input_file: String,
    },

    #[structopt(name = "import-desktop-bookmarks")]
    /// Import bookmarks from JSON file exported by desktop Firefox
    ImportDesktopBookmarks {
        #[structopt(name = "input-file", long, short = "i")]
        /// Imports bookmarks from a desktop export
        input_file: String,
    },

    #[structopt(name = "create-fake-visits")]
    /// Create a lot of fake visits to a lot of fake sites.
    CreateFakeVisits {
        #[structopt(name = "num-sites", long)]
        /// The number of `exampleX.com` sites to use.
        num_sites: usize,
        #[structopt(name = "num-visits", long)]
        /// The number of visits per site to create
        num_visits: usize,
    },

    #[structopt(name = "delete-history")]
    /// Remove history
    DeleteHistory,

    #[structopt(name = "run-maintenance")]
    /// Run maintenance on the database
    RunMaintenance {
        #[structopt(name = "db-size-limit", long, default_value = "75000000")]
        /// Target size of the database (in bytes)
        db_size_limit: u32,
        #[structopt(name = "count", long, short = "c", default_value = "1")]
        /// Repeat the operation N times
        count: u32,
    },

    #[structopt(name = "show-stats")]
    /// Show statistics about the database
    ShowStats,
}

fn main() -> Result<()> {
    let opts = Opts::from_args();
    if !opts.no_logging {
        cli_support::init_trace_logging();
    }

    let db_path = opts.database_path;
    let api = PlacesApi::new(db_path)?;
    let db = api.open_connection(ConnectionType::ReadWrite)?;
    // Needed to make the get_registered_sync_engine() calls work.
    std::sync::Arc::clone(&api).register_with_sync_manager();

    ctrlc::set_handler(move || {
        println!("\nCTRL-C detected, enabling shutdown mode\n");
        interrupt_support::shutdown();
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
        Command::ExportBookmarks { output_file } => run_native_export(&db, output_file),
        Command::ImportBookmarks { input_file } => run_native_import(&db, input_file),
        Command::ImportDesktopBookmarks { input_file } => run_desktop_import(&db, input_file),
        Command::ImportIosHistory { input_file } => run_ios_import_history(&db, input_file),
        Command::CreateFakeVisits {
            num_sites,
            num_visits,
        } => create_fake_visits(&db, num_sites, num_visits),
        Command::DeleteHistory => delete_history(&db),
        Command::RunMaintenance {
            db_size_limit,
            count,
        } => run_maintenance(&db, db_size_limit, count),
        Command::ShowStats => show_stats(&db),
    }
}

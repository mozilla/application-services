/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use dump::client::CollectionDownloader;
use remote_settings::{RemoteSettingsConfig2, RemoteSettingsServer, RemoteSettingsService};

const DEFAULT_LOG_FILTER: &str = "remote_settings=info";
const DEFAULT_LOG_FILTER_VERBOSE: &str = "remote_settings=trace";

#[derive(Debug, Parser)]
#[command(about, long_about = None)]
struct Cli {
    #[arg(short = 's')]
    server: Option<RemoteSettingsServerArg>,
    #[arg(short = 'b')]
    bucket: Option<String>,
    #[arg(short = 'd')]
    storage_dir: Option<String>,
    #[arg(long, short, action)]
    verbose: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Debug, ValueEnum)]
enum RemoteSettingsServerArg {
    Prod,
    Stage,
    Dev,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Sync collections
    Sync {
        #[clap(required = true)]
        collections: Vec<String>,
    },
    /// Query against ingested data
    Get {
        collection: String,
        #[arg(long)]
        sync_if_empty: bool,
    },
    /// Download and combine all remote settings collections
    DumpSync {
        /// Root path of the repository
        #[arg(short, long, default_value = ".")]
        path: PathBuf,

        /// Dry run - don't write any files
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Download a single collection to the dumps directory
    DumpGet {
        /// Bucket name
        #[arg(long, required = true)]
        bucket: String,

        /// Collection name
        #[arg(long, required = true)]
        collection_name: String,

        /// Root path of the repository
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    env_logger::init_from_env(env_logger::Env::default().filter_or(
        "RUST_LOG",
        if cli.verbose {
            DEFAULT_LOG_FILTER_VERBOSE
        } else {
            DEFAULT_LOG_FILTER
        },
    ));
    nss::ensure_initialized();
    viaduct_reqwest::use_reqwest_backend();
    let service = build_service(&cli)?;
    match cli.command {
        Commands::Sync { collections } => sync(service, collections),
        Commands::Get {
            collection,
            sync_if_empty,
        } => {
            get_records(service, collection, sync_if_empty);
            Ok(())
        }
        Commands::DumpSync { path, dry_run } => {
            let downloader = CollectionDownloader::new(path);
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(downloader.run(dry_run))
        }
        Commands::DumpGet {
            bucket,
            collection_name,
            path,
        } => {
            let downloader = CollectionDownloader::new(path);
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(downloader.download_single(&bucket, &collection_name))
        }
    }
}

fn build_service(cli: &Cli) -> Result<RemoteSettingsService> {
    let config = RemoteSettingsConfig2 {
        server: cli.server.as_ref().map(|s| match s {
            RemoteSettingsServerArg::Dev => RemoteSettingsServer::Dev,
            RemoteSettingsServerArg::Stage => RemoteSettingsServer::Stage,
            RemoteSettingsServerArg::Prod => RemoteSettingsServer::Prod,
        }),
        bucket_name: cli.bucket.clone(),
        app_context: None,
    };
    cli_support::ensure_cli_data_dir_exists();
    let storage_dir = cli
        .storage_dir
        .clone()
        .unwrap_or_else(|| cli_support::cli_data_subdir("remote-settings-data"));
    Ok(RemoteSettingsService::new(storage_dir, config))
}

fn sync(service: RemoteSettingsService, collections: Vec<String>) -> Result<()> {
    // Create a bunch of clients so that sync() syncs their collections
    let _clients = collections
        .into_iter()
        .map(|collection| service.make_client(collection))
        .collect::<Vec<_>>();
    service.sync()?;
    Ok(())
}

fn get_records(service: RemoteSettingsService, collection: String, sync_if_empty: bool) {
    let client = service.make_client(collection);
    match client.get_records(sync_if_empty) {
        Some(records) => {
            for record in records {
                println!("{record:?}");
            }
        }
        None => println!("No cached records"),
    }
}

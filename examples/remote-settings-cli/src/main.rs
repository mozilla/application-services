/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

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
    viaduct_reqwest::use_reqwest_backend();
    let service = build_service(&cli)?;
    match cli.command {
        Commands::Sync { collections } => sync(service, collections),
        Commands::Get {
            collection,
            sync_if_empty,
        } => get_records(service, collection, sync_if_empty),
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
    };
    Ok(RemoteSettingsService::new(
        cli.storage_dir
            .clone()
            .unwrap_or_else(|| "remote-settings-data".into()),
        config,
    )?)
}

fn sync(service: RemoteSettingsService, collections: Vec<String>) -> Result<()> {
    // Create a bunch of clients so that sync() syncs their collections
    let _clients = collections
        .into_iter()
        .map(|collection| Ok(service.make_client(collection, None)?))
        .collect::<Result<Vec<_>>>()?;
    service.sync()?;
    Ok(())
}

fn get_records(
    service: RemoteSettingsService,
    collection: String,
    sync_if_empty: bool,
) -> Result<()> {
    let client = service.make_client(collection, None)?;
    match client.get_records(sync_if_empty) {
        Some(records) => {
            for record in records {
                println!("{record:?}");
            }
        }
        None => println!("No cached records"),
    }
    Ok(())
}

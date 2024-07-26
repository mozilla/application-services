/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use remote_settings::RemoteSettingsServer;
use suggest::{
    SuggestIngestionConstraints, SuggestStore, SuggestStoreBuilder, SuggestionProvider,
    SuggestionQuery,
};

static DB_PATH: &str = "suggest.db";

const DEFAULT_LOG_FILTER: &str = "suggest::store=info";
const DEFAULT_LOG_FILTER_VERBOSE: &str = "suggest::store=trace";

#[derive(Debug, Parser)]
#[command(about, long_about = None)]
struct Cli {
    #[arg(short = 's')]
    remote_settings_server: Option<RemoteSettingsServerArg>,
    #[arg(short = 'b')]
    remote_settings_bucket: Option<String>,
    #[arg(long, short, action)]
    verbose: bool,
    // Custom { url: String },
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
    /// Ingest data
    Ingest {
        #[clap(long, short, action)]
        reingest: bool,
        #[clap(long, short)]
        providers: Vec<SuggestionProviderArg>,
    },
    /// Query against ingested data
    Query {
        provider: SuggestionProviderArg,
        /// Input to search
        input: String,
    },
}

#[derive(Clone, Debug, ValueEnum)]
enum SuggestionProviderArg {
    Amp,
    Wikipedia,
    Amo,
    Pocket,
    Yelp,
    Mdn,
    Weather,
    AmpMobile,
    Fakespot,
}

impl From<SuggestionProviderArg> for SuggestionProvider {
    fn from(value: SuggestionProviderArg) -> Self {
        match value {
            SuggestionProviderArg::Amp => Self::Amp,
            SuggestionProviderArg::Wikipedia => Self::Wikipedia,
            SuggestionProviderArg::Amo => Self::Amo,
            SuggestionProviderArg::Pocket => Self::Pocket,
            SuggestionProviderArg::Yelp => Self::Yelp,
            SuggestionProviderArg::Mdn => Self::Mdn,
            SuggestionProviderArg::Weather => Self::Weather,
            SuggestionProviderArg::AmpMobile => Self::AmpMobile,
            SuggestionProviderArg::Fakespot => Self::Fakespot,
        }
    }
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
    let store = build_store(&cli);
    match cli.command {
        Commands::Ingest {
            reingest,
            providers,
        } => ingest(&store, reingest, providers),
        Commands::Query { provider, input } => query(&store, provider, input),
    };
    Ok(())
}

fn build_store(cli: &Cli) -> Arc<SuggestStore> {
    Arc::new(SuggestStoreBuilder::default())
        .data_path(DB_PATH.to_string())
        .remote_settings_server(match cli.remote_settings_server {
            None => RemoteSettingsServer::Prod,
            Some(RemoteSettingsServerArg::Dev) => RemoteSettingsServer::Dev,
            Some(RemoteSettingsServerArg::Stage) => RemoteSettingsServer::Stage,
            Some(RemoteSettingsServerArg::Prod) => RemoteSettingsServer::Prod,
        })
        .remote_settings_bucket_name(
            cli.remote_settings_bucket
                .clone()
                .unwrap_or_else(|| "main".to_owned()),
        )
        .build()
        .unwrap_or_else(|e| panic!("Error building store: {e}"))
}

fn ingest(store: &SuggestStore, reingest: bool, providers: Vec<SuggestionProviderArg>) {
    if reingest {
        println!("Reingesting data...");
        store.force_reingest();
    } else {
        println!("Ingesting data...");
    }
    let constraints = if providers.is_empty() {
        SuggestIngestionConstraints::all_providers()
    } else {
        SuggestIngestionConstraints {
            providers: Some(providers.into_iter().map(Into::into).collect()),
            ..SuggestIngestionConstraints::default()
        }
    };

    store
        .ingest(constraints)
        .unwrap_or_else(|e| panic!("Error in ingest: {e}"));
    println!("Done");
}

fn query(store: &SuggestStore, provider: SuggestionProviderArg, input: String) {
    let query = SuggestionQuery {
        providers: vec![provider.into()],
        keyword: input,
        limit: None,
    };
    let suggestions = store
        .query(query)
        .unwrap_or_else(|e| panic!("Error querying store: {e}"));
    if suggestions.is_empty() {
        println!("No Results");
    } else {
        println!("Results:");
        for suggestion in suggestions {
            let title = suggestion.title();
            let url = suggestion.url().unwrap_or("[no-url]");
            let icon = if suggestion.icon_data().is_some() {
                "with icon"
            } else {
                "no icon"
            };
            println!("{title} ({url}) ({icon})");
        }
    }
}

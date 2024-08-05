/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{collections::HashMap, sync::Arc};

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
        #[clap(long, short)]
        provider: Option<SuggestionProviderArg>,
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
        } => ingest(&store, reingest, providers, cli.verbose),
        Commands::Query { provider, input } => query(&store, provider, input, cli.verbose),
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

fn ingest(
    store: &SuggestStore,
    reingest: bool,
    providers: Vec<SuggestionProviderArg>,
    verbose: bool,
) {
    if reingest {
        print_header("Reingesting data");
        store.force_reingest();
    } else {
        print_header("Ingesting data");
    }
    let constraints = if providers.is_empty() {
        SuggestIngestionConstraints::all_providers()
    } else {
        SuggestIngestionConstraints {
            providers: Some(providers.into_iter().map(Into::into).collect()),
            ..SuggestIngestionConstraints::default()
        }
    };

    let metrics = store
        .ingest(constraints)
        .unwrap_or_else(|e| panic!("Error in ingest: {e}"));
    if verbose {
        print_header("Ingestion times");
        let mut ingestion_times = metrics.ingestion_times;
        let download_times: HashMap<String, u64> = metrics
            .download_times
            .into_iter()
            .map(|s| (s.label, s.value))
            .collect();

        ingestion_times.sort_by_key(|s| s.value);
        ingestion_times.reverse();
        for sample in ingestion_times {
            let label = &sample.label;
            let ingestion_time = sample.value / 1000;
            let download_time = download_times.get(label).unwrap_or(&0) / 1000;

            println!(
                "{label:30} Download: {download_time:>5}ms    Ingestion: {ingestion_time:>5}ms"
            );
        }
    }
    print_header("Done");
}

fn query(
    store: &SuggestStore,
    provider: Option<SuggestionProviderArg>,
    input: String,
    verbose: bool,
) {
    let query = SuggestionQuery {
        providers: match provider {
            Some(provider) => vec![provider.into()],
            None => SuggestionProvider::all().to_vec(),
        },
        keyword: input,
        limit: None,
    };
    let mut results = store
        .query_with_metrics(query)
        .unwrap_or_else(|e| panic!("Error querying store: {e}"));
    if results.suggestions.is_empty() {
        print_header("No Results");
    } else {
        print_header("Results");
        for suggestion in results.suggestions {
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
    if verbose {
        print_header("Query times");
        results.query_times.sort_by_key(|s| s.value);
        results.query_times.reverse();
        for s in results.query_times {
            println!("{:33} Time: {:>5}us", s.label, s.value);
        }
    }
}

fn print_header(msg: impl Into<String>) {
    let mut msg = msg.into();
    if msg.len() % 2 == 1 {
        msg.push(' ');
    }
    let width = (70 - msg.len() - 2) / 2;
    println!();
    println!("{} {msg} {}", "=".repeat(width), "=".repeat(width));
}

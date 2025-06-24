/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use remote_settings::{RemoteSettingsConfig2, RemoteSettingsServer, RemoteSettingsService};
use suggest::{
    AmpMatchingStrategy, SuggestIngestionConstraints, SuggestStore, SuggestStoreBuilder,
    SuggestionProvider, SuggestionProviderConstraints, SuggestionQuery,
};

static DB_FILENAME: &str = "suggest.db";

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
        #[arg(long, action)]
        fts_match_info: bool,
        #[clap(long, short)]
        provider: Option<SuggestionProviderArg>,
        /// Input to search
        input: String,
        #[clap(long, short)]
        amp_matching_strategy: Option<AmpMatchingStrategyArg>,
    },
}

#[derive(Clone, Debug, ValueEnum)]
enum AmpMatchingStrategyArg {
    /// Use keyword matching, without keyword expansion
    NoKeyword = 1, // Use `1` as the starting discriminant, since the JS code assumes this.
    /// Use FTS matching
    Fts,
    /// Use FTS matching against the title
    FtsTitle,
}

impl From<AmpMatchingStrategyArg> for AmpMatchingStrategy {
    fn from(val: AmpMatchingStrategyArg) -> Self {
        match val {
            AmpMatchingStrategyArg::NoKeyword => AmpMatchingStrategy::NoKeywordExpansion,
            AmpMatchingStrategyArg::Fts => AmpMatchingStrategy::FtsAgainstFullKeywords,
            AmpMatchingStrategyArg::FtsTitle => AmpMatchingStrategy::FtsAgainstTitle,
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
enum SuggestionProviderArg {
    Amp,
    Wikipedia,
    Amo,
    Yelp,
    Mdn,
    Weather,
    Fakespot,
}

impl From<SuggestionProviderArg> for SuggestionProvider {
    fn from(value: SuggestionProviderArg) -> Self {
        match value {
            SuggestionProviderArg::Amp => Self::Amp,
            SuggestionProviderArg::Wikipedia => Self::Wikipedia,
            SuggestionProviderArg::Amo => Self::Amo,
            SuggestionProviderArg::Yelp => Self::Yelp,
            SuggestionProviderArg::Mdn => Self::Mdn,
            SuggestionProviderArg::Weather => Self::Weather,
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
    nss::ensure_initialized();
    viaduct_reqwest::use_reqwest_backend();
    let store = build_store(&cli)?;
    match cli.command {
        Commands::Ingest {
            reingest,
            providers,
        } => ingest(&store, reingest, providers, cli.verbose),
        Commands::Query {
            provider,
            input,
            fts_match_info,
            amp_matching_strategy,
        } => query(
            &store,
            provider,
            input,
            fts_match_info,
            amp_matching_strategy,
            cli.verbose,
        ),
    };
    Ok(())
}

fn build_store(cli: &Cli) -> Result<Arc<SuggestStore>> {
    Ok(Arc::new(SuggestStoreBuilder::default())
        .data_path(cli_support::cli_data_path(DB_FILENAME))
        .remote_settings_service(build_remote_settings_service(cli))
        .build()?)
}

fn build_remote_settings_service(cli: &Cli) -> Arc<RemoteSettingsService> {
    let config = RemoteSettingsConfig2 {
        server: cli.remote_settings_server.as_ref().map(|s| match s {
            RemoteSettingsServerArg::Dev => RemoteSettingsServer::Dev,
            RemoteSettingsServerArg::Stage => RemoteSettingsServer::Stage,
            RemoteSettingsServerArg::Prod => RemoteSettingsServer::Prod,
        }),
        bucket_name: cli.remote_settings_bucket.clone(),
        app_context: None,
    };
    let storage_dir = cli_support::cli_data_subdir("remote-settings-data");
    Arc::new(RemoteSettingsService::new(storage_dir, config))
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
    if verbose && !metrics.ingestion_times.is_empty() {
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
    fts_match_info: bool,
    amp_matching_strategy: Option<AmpMatchingStrategyArg>,
    verbose: bool,
) {
    let query = SuggestionQuery {
        providers: match provider {
            Some(provider) => vec![provider.into()],
            None => SuggestionProvider::all().to_vec(),
        },
        keyword: input,
        provider_constraints: Some(SuggestionProviderConstraints {
            amp_alternative_matching: amp_matching_strategy.map(Into::into),
            ..SuggestionProviderConstraints::default()
        }),
        ..SuggestionQuery::default()
    };
    let mut results = store
        .query_with_metrics(query)
        .unwrap_or_else(|e| panic!("Error querying store: {e}"));
    if results.suggestions.is_empty() {
        print_header("No Results");
    } else {
        print_header("Results");
        let count = results.suggestions.len();
        for suggestion in results.suggestions {
            let title = suggestion.title();
            let url = suggestion.url().unwrap_or("[no-url]");
            let icon = if suggestion.icon_data().is_some() {
                "with icon"
            } else {
                "no icon"
            };
            println!("* {title} ({url}) ({icon})");
            if fts_match_info {
                if let Some(match_info) = suggestion.fts_match_info() {
                    println!("   {match_info:?}")
                } else {
                    println!("   <no match info>");
                }
                println!("+ {} other sugestions", count - 1);
                break;
            }
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

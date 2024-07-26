/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    benchmarks::{
        client::{RemoteSettingsBenchmarkClient, RemoteSettingsWarmUpClient},
        unique_db_filename, BenchmarkWithInput,
    },
    provider::SuggestionProvider,
    store::SuggestStoreInner,
    SuggestIngestionConstraints,
};

pub struct IngestBenchmark {
    temp_dir: tempfile::TempDir,
    client: RemoteSettingsBenchmarkClient,
    provider: SuggestionProvider,
    reingest: bool,
}

impl IngestBenchmark {
    pub fn new(provider: SuggestionProvider, reingest: bool) -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SuggestStoreInner::new(
            temp_dir.path().join("warmup.sqlite"),
            vec![],
            RemoteSettingsWarmUpClient::new(),
        );
        store.benchmark_fetch_and_ingest_records(provider);
        Self {
            client: RemoteSettingsBenchmarkClient::from(store.into_settings_client()),
            temp_dir,
            provider,
            reingest,
        }
    }
}

// The input for each benchmark is `SuggestStoreInner` with a fresh database.
//
// This is wrapped in a newtype so that it can be exposed in the public trait
pub struct InputType(SuggestStoreInner<RemoteSettingsBenchmarkClient>);

impl BenchmarkWithInput for IngestBenchmark {
    type Input = InputType;

    fn generate_input(&self) -> Self::Input {
        let data_path = self.temp_dir.path().join(unique_db_filename());
        let store = SuggestStoreInner::new(data_path, vec![], self.client.clone());
        store.ensure_db_initialized();
        if self.reingest {
            store.force_reingest(self.provider);
        }
        InputType(store)
    }

    fn benchmarked_code(&self, input: Self::Input) {
        let InputType(store) = input;
        store.benchmark_fetch_and_ingest_records(self.provider);
    }
}

/// Get IngestBenchmark instances for all record types
pub fn all_benchmarks() -> Vec<(&'static str, IngestBenchmark)> {
    vec![
        (
            "ingest-amp",
            IngestBenchmark::new(SuggestionProvider::Amp, false),
        ),
        (
            "ingest-again-amp",
            IngestBenchmark::new(SuggestionProvider::Amp, true),
        ),
        (
            "ingest-wikipedia",
            IngestBenchmark::new(SuggestionProvider::Wikipedia, false),
        ),
        (
            "ingest-again-wikipedia",
            IngestBenchmark::new(SuggestionProvider::Wikipedia, true),
        ),
        (
            "ingest-amo",
            IngestBenchmark::new(SuggestionProvider::Amo, false),
        ),
        (
            "ingest-again-amo",
            IngestBenchmark::new(SuggestionProvider::Amo, true),
        ),
        (
            "ingest-pocket",
            IngestBenchmark::new(SuggestionProvider::Pocket, false),
        ),
        (
            "ingest-again-pocket",
            IngestBenchmark::new(SuggestionProvider::Pocket, true),
        ),
        (
            "ingest-yelp",
            IngestBenchmark::new(SuggestionProvider::Yelp, false),
        ),
        (
            "ingest-again-yelp",
            IngestBenchmark::new(SuggestionProvider::Yelp, true),
        ),
        (
            "ingest-mdn",
            IngestBenchmark::new(SuggestionProvider::Mdn, false),
        ),
        (
            "ingest-again-mdn",
            IngestBenchmark::new(SuggestionProvider::Mdn, true),
        ),
        (
            "ingest-weather",
            IngestBenchmark::new(SuggestionProvider::Weather, false),
        ),
        (
            "ingest-again-weather",
            IngestBenchmark::new(SuggestionProvider::Weather, true),
        ),
        (
            "ingest-amp-mobile",
            IngestBenchmark::new(SuggestionProvider::AmpMobile, false),
        ),
        (
            "ingest-again-amp-mobile",
            IngestBenchmark::new(SuggestionProvider::AmpMobile, true),
        ),
        (
            "ingest-fakespot",
            IngestBenchmark::new(SuggestionProvider::Fakespot, false),
        ),
        (
            "ingest-again-fakespot",
            IngestBenchmark::new(SuggestionProvider::Fakespot, true),
        ),
    ]
}

pub fn print_debug_ingestion_sizes() {
    viaduct_reqwest::use_reqwest_backend();
    let store = SuggestStoreInner::new(
        "file:debug_ingestion_sizes?mode=memory&cache=shared",
        vec![],
        RemoteSettingsWarmUpClient::new(),
    );
    store
        .ingest(SuggestIngestionConstraints {
            // Uncomment to measure the size for a specific provider
            // providers: Some(vec![crate::SuggestionProvider::Fakespot]),
            ..SuggestIngestionConstraints::default()
        })
        .unwrap();
    let table_row_counts = store.table_row_counts();
    let db_size = store.db_size();
    let client = store.into_settings_client();
    let total_attachment_size: usize = client
        .get_records_responses
        .lock()
        .values()
        .flat_map(|records| {
            records.iter().map(|r| match &r.attachment_data {
                Some(d) => d.len(),
                None => 0,
            })
        })
        .sum();

    println!(
        "Total attachment size: {}kb",
        (total_attachment_size + 500) / 1000
    );
    println!("Total database size: {}kb", (db_size + 500) / 1000);
    println!();
    println!("Database table row counts");
    println!("-------------------------");
    for (name, count) in table_row_counts {
        println!("{name:30}: {count}");
    }
}

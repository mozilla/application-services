/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{
    benchmarks::{
        client::{RemoteSettingsBenchmarkClient, RemoteSettingsWarmUpClient},
        BenchmarkWithInput,
    },
    rs::SuggestRecordType,
    store::SuggestStoreInner,
};
use std::sync::atomic::{AtomicU32, Ordering};

static DB_FILE_COUNTER: AtomicU32 = AtomicU32::new(0);

pub struct IngestBenchmark {
    temp_dir: tempfile::TempDir,
    client: RemoteSettingsBenchmarkClient,
    record_type: SuggestRecordType,
}

impl IngestBenchmark {
    pub fn new(record_type: SuggestRecordType) -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = SuggestStoreInner::new(
            temp_dir.path().join("warmup.sqlite"),
            RemoteSettingsWarmUpClient::new(),
        );
        store.benchmark_ingest_records_by_type(record_type);
        Self {
            client: RemoteSettingsBenchmarkClient::from(store.into_settings_client()),
            temp_dir,
            record_type,
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
        let data_path = self.temp_dir.path().join(format!(
            "db{}.sqlite",
            DB_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let store = SuggestStoreInner::new(data_path, self.client.clone());
        store.ensure_db_initialized();
        InputType(store)
    }

    fn benchmarked_code(&self, input: Self::Input) {
        let InputType(store) = input;
        store.benchmark_ingest_records_by_type(self.record_type);
    }
}

/// Get IngestBenchmark instances for all record types
pub fn all_benchmarks() -> Vec<(&'static str, IngestBenchmark)> {
    vec![
        ("icon", IngestBenchmark::new(SuggestRecordType::Icon)),
        (
            "amp-wikipedia",
            IngestBenchmark::new(SuggestRecordType::AmpWikipedia),
        ),
        ("amo", IngestBenchmark::new(SuggestRecordType::Amo)),
        ("pocket", IngestBenchmark::new(SuggestRecordType::Pocket)),
        ("yelp", IngestBenchmark::new(SuggestRecordType::Yelp)),
        ("mdn", IngestBenchmark::new(SuggestRecordType::Mdn)),
        ("weather", IngestBenchmark::new(SuggestRecordType::Weather)),
        (
            "global-config",
            IngestBenchmark::new(SuggestRecordType::GlobalConfig),
        ),
        (
            "amp-mobile",
            IngestBenchmark::new(SuggestRecordType::AmpMobile),
        ),
    ]
}

# Suggest benchmarking code

Use `cargo suggest-bench` to run these benchmarks.

The main benchmarking code lives here, while the criterion integration code lives in the `benches/`
directory.

## Benchmarks

### ingest-[provider-type]

Time it takes to ingest all suggestions for a provider type on an empty database.
The bechmark downloads network resources in advance in order to exclude the network request time
from these measurements.

### Benchmarks it would be nice to have

- Ingestion with synthetic data.  This would isolate the benchmark from changes to the RS database.
- Fetching suggestions

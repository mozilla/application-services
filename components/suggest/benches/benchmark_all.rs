use criterion::{
    criterion_group, criterion_main, measurement::Measurement, BatchSize, BenchmarkGroup, Criterion,
};
use std::sync::Once;
use suggest::benchmarks::{ingest, query, BenchmarkWithInput};

pub fn ingest(c: &mut Criterion) {
    setup_viaduct();
    let mut group = c.benchmark_group("ingest");
    // This needs to be 10 for now, or else the `ingest-amp-wikipedia` benchmark would take around
    // 100s to run which feels like too long.  `ingest-amp-mobile` also would take a around 50s.
    group.sample_size(10);
    run_benchmarks(group, ingest::all_benchmarks())
}

pub fn query(c: &mut Criterion) {
    setup_viaduct();
    let group = c.benchmark_group("query");
    run_benchmarks(group, query::all_benchmarks())
}

fn run_benchmarks<B: BenchmarkWithInput, M: Measurement>(
    mut group: BenchmarkGroup<M>,
    benchmarks: Vec<(&'static str, B)>,
) {
    for (name, benchmark) in benchmarks {
        group.bench_function(name.to_string(), |b| {
            b.iter_batched(
                || benchmark.generate_input(),
                |input| benchmark.benchmarked_code(input),
                // See https://docs.rs/criterion/latest/criterion/enum.BatchSize.html#variants for
                // a discussion of this.  PerIteration is chosen for these benchmarks because the
                // input holds a database file handle
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn setup_viaduct() {
    static INIT: Once = Once::new();
    INIT.call_once(viaduct_reqwest::use_reqwest_backend);
}

criterion_group!(benches, ingest, query);
criterion_main!(benches);

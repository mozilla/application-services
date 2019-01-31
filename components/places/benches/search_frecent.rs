use criterion::{criterion_group, criterion_main, Criterion};

use places::api::matcher::{search_frecent, SearchParams};
use places::PlacesDb;
use std::rc::Rc;
use tempdir::TempDir;

#[derive(Clone, Debug, serde_derive::Deserialize)]
struct DummyHistoryEntry {
    url: String,
    title: String,
}
fn init_db(db: &mut PlacesDb) -> places::Result<()> {
    let dummy_data = include_str!("../fixtures/dummy_urls.json");
    let entries: Vec<DummyHistoryEntry> = serde_json::from_str(dummy_data)?;
    let tx = db.db.transaction()?;
    let day_ms = 24 * 60 * 60 * 1000;
    let now: places::Timestamp = std::time::SystemTime::now().into();
    for entry in entries {
        let url = url::Url::parse(&entry.url).unwrap();
        for i in 0..20 {
            let obs = places::VisitObservation::new(url.clone())
                .with_title(entry.title.clone())
                .with_is_remote(i < 10)
                .with_visit_type(places::VisitTransition::Link)
                .with_at(places::Timestamp(now.0 - day_ms * (1 + i)));
            places::storage::history::apply_observation_direct(&tx, obs)?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn bench_search_frecent(c: &mut Criterion) {
    let dbdir = Rc::new(TempDir::new("placesbench").unwrap());
    let dbfile = dbdir.path().join("places.sqlite");
    let mut db = PlacesDb::open(&dbfile, None).unwrap();
    init_db(&mut db).unwrap();
    let db = Rc::new(db);
    {
        let db = db.clone();
        let dir = dbdir.clone();
        c.bench_function("search_frecent string", move |b| {
            let _dir = dir.clone(); // ensure it stays alive...
            let params = SearchParams {
                search_string: "mozilla".into(),
                limit: 10,
            };
            b.iter(|| search_frecent(&db, params.clone()).unwrap())
        });
    }
    {
        let db = db.clone();
        let dir = dbdir.clone();
        c.bench_function("search_frecent origin", move |b| {
            let _dir = dir.clone(); // ensure it stays alive...
            let params = SearchParams {
                search_string: "blog.mozilla.org".into(),
                limit: 10,
            };
            b.iter(|| search_frecent(&db, params.clone()).unwrap())
        });
    }
    {
        let db = db.clone();
        let dir = dbdir.clone();
        c.bench_function("search_frecent url", move |b| {
            let _dir = dir.clone(); // ensure it stays alive...
            let params = SearchParams {
                search_string: "https://hg.mozilla.org/mozilla-central".into(),
                limit: 10,
            };
            b.iter(|| search_frecent(&db, params.clone()).unwrap())
        });
    }
}

criterion_group!(benches, bench_search_frecent);
criterion_main!(benches);

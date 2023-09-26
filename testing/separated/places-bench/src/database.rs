/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use criterion::Criterion;
use places::api::{
    matcher::{match_url, search_frecent, SearchParams},
    places_api::ConnectionType,
};
use places::PlacesDb;
use sql_support::ConnExt;
use std::rc::Rc;
use types::Timestamp;

#[derive(Clone, Debug)]
struct DummyHistoryEntry {
    url: String,
    title: String,
}
fn get_dummy_data() -> Vec<DummyHistoryEntry> {
    let dummy_data = include_str!("../fixtures/dummy_urls.json");
    let entries: Vec<serde_json::Value> = serde_json::from_str(dummy_data).unwrap();
    entries
        .into_iter()
        .map(|m| DummyHistoryEntry {
            url: m["url"].as_str().unwrap().into(),
            title: m["title"].as_str().unwrap().into(),
        })
        .collect()
}

fn init_db(db: &mut PlacesDb) -> places::Result<()> {
    let tx = db.unchecked_transaction()?;
    let entries = get_dummy_data();
    let day_ms = 24 * 60 * 60 * 1000;
    let now: Timestamp = std::time::SystemTime::now().into();
    for entry in entries {
        let url = url::Url::parse(&entry.url).unwrap();
        for i in 0..20 {
            let obs = places::VisitObservation::new(url.clone())
                .with_title(entry.title.clone())
                .with_is_remote(i < 10)
                .with_visit_type(places::VisitType::Link)
                .with_at(Timestamp(now.0 - day_ms * (1 + i)));
            places::storage::history::apply_observation_direct(db, obs)?;
        }
    }
    places::storage::delete_pending_temp_tables(db)?;
    tx.commit()?;
    Ok(())
}

pub struct TestDb {
    // Needs to be here so that the dir isn't deleted.
    _dir: tempfile::TempDir,
    pub db: PlacesDb,
}

impl TestDb {
    pub fn new() -> Rc<Self> {
        use std::sync::Arc;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("places.sqlite");
        let mut db = PlacesDb::open(
            file,
            ConnectionType::ReadWrite,
            0,
            Arc::new(parking_lot::Mutex::new(())),
        )
        .unwrap();
        println!("Populating test database...");
        init_db(&mut db).unwrap();
        println!("Done populating test db");
        Rc::new(Self { _dir: dir, db })
    }
}

macro_rules! db_bench {
    ($c:expr, $name:literal, |$db:ident : $test_db_name:ident| $expr:expr) => {{
        let $test_db_name = $test_db_name.clone();
        $c.bench_function($name, move |b| {
            let $db = &$test_db_name.db;
            b.iter(|| $expr)
        });
    }};
}

pub fn bench_search_frecent(c: &mut Criterion) {
    let test_db = TestDb::new();
    db_bench!(c, "search_frecent string", |db: test_db| {
        search_frecent(
            db,
            SearchParams {
                search_string: "mozilla".into(),
                limit: 10,
            },
        )
        .unwrap()
    });
    db_bench!(c, "search_frecent origin", |db: test_db| {
        search_frecent(
            db,
            SearchParams {
                search_string: "blog.mozilla.org".into(),
                limit: 10,
            },
        )
        .unwrap()
    });
    db_bench!(c, "search_frecent url", |db: test_db| {
        search_frecent(
            db,
            SearchParams {
                search_string: "https://hg.mozilla.org/mozilla-central".into(),
                limit: 10,
            },
        )
        .unwrap()
    });
}

pub fn bench_match_url(c: &mut Criterion) {
    let test_db = TestDb::new();
    db_bench!(c, "match_url string", |db: test_db| {
        match_url(db, "mozilla").unwrap()
    });
    db_bench!(c, "match_url origin", |db: test_db| {
        match_url(db, "blog.mozilla.org").unwrap()
    });
    db_bench!(c, "match_url url", |db: test_db| {
        match_url(db, "https://hg.mozilla.org/mozilla-central").unwrap()
    });
}

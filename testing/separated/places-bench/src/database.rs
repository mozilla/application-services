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

/*
 * Benchmarking speeding up fetch_outgoing by adding a partial index
*/

// A helper function that benches can use to validate what queries are doing
fn explain(name: String, conn: &PlacesDb, sql_template: &str, limit: i64) {
    // Replace the {LIMIT} placeholder with the actual limit value
    let sql = sql_template.replace("{LIMIT}", &limit.to_string());

    let mut stmt = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {}", sql))
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            // No parameters needed since we already substituted
            Ok((
                r.get::<_, i64>(0)?,    // id
                r.get::<_, i64>(1)?,    // parent
                r.get::<_, i64>(2)?,    // notused
                r.get::<_, String>(3)?, // detail
            ))
        })
        .unwrap();
    println!("--- EXPLAIN QUERY PLAN {} for limit {} ---", name, limit);
    for row in rows {
        let (id, parent, _notused, detail) = row.unwrap();
        println!("  {}-{}: {}", id, parent, detail);
    }
    println!("--- END EXPLAIN ---\n");
}

// Original (slow) predicate seen in https://bugzilla.mozilla.org/show_bug.cgi?id=1979764
const OUTGOING_SQL_ORIGINAL: &str = r#"
    SELECT guid, url, id, title, hidden, typed, frecency,
           visit_count_local, visit_count_remote,
           last_visit_date_local, last_visit_date_remote,
           sync_status, sync_change_counter, preview_image_url,
           unknown_fields
    FROM moz_places
    WHERE (sync_change_counter > 0 OR sync_status != 2) AND
          NOT hidden
    ORDER BY frecency DESC
    LIMIT {LIMIT}
"#;

// Changing hidden = 0, instead of NOT allows us to be index-friendly:
const OUTGOING_SQL_INDEX_FRIENDLY: &str = r#"
    SELECT guid, url, id, title, hidden, typed, frecency,
           visit_count_local, visit_count_remote,
           last_visit_date_local, last_visit_date_remote,
           sync_status, sync_change_counter, preview_image_url,
           unknown_fields
    FROM moz_places
    WHERE hidden = 0 AND (sync_change_counter > 0 OR sync_status != 2)
    ORDER BY frecency DESC
    LIMIT {LIMIT}
"#;

/// Flip a subset of rows to be "changed", and mark some as hidden.
/// This ensures both WHERE branches return work and the partial index has a
/// selective predicate.
fn seed_outgoing_flags(db: &PlacesDb) -> places::Result<()> {
    // majority "Normal"
    db.execute(
        "UPDATE moz_places
         SET sync_status = CASE WHEN (id % 50) = 0 THEN 1 ELSE 2 END", // some non-normal
        [],
    )?;

    // Small minority with local changes
    db.execute(
        "UPDATE moz_places
         SET sync_change_counter = CASE WHEN (id % 20) = 0 THEN 1 ELSE 0 END", // ~5% changed
        [],
    )?;
    // Some hidden entries
    db.execute(
        "UPDATE moz_places
         SET hidden = CASE WHEN (id % 7) = 0 THEN 1 ELSE 0 END", // ~14% hidden
        [],
    )?;
    Ok(())
}

fn drop_outgoing_partial_index(db: &PlacesDb) -> places::Result<()> {
    db.execute("DROP INDEX IF EXISTS idx_places_outgoing_by_frecency", [])?;
    // Recompute stats so planner isn't biased by previous runs.
    db.execute("ANALYZE", [])?;
    Ok(())
}

fn create_outgoing_partial_index(db: &PlacesDb) -> places::Result<()> {
    // DESC on frecency helps the ORDER BY; Partial predicate matches WHERE.
    db.execute(
        "CREATE INDEX IF NOT EXISTS idx_places_outgoing_by_frecency
         ON moz_places(frecency DESC)
         WHERE hidden = 0 AND (sync_change_counter > 0 OR sync_status != 2)",
        [],
    )?;
    db.execute("ANALYZE", [])?;
    Ok(())
}

fn run_outgoing_query(
    db: &PlacesDb,
    use_index_friendly_where: bool,
    limit: usize,
) -> places::Result<usize> {
    let sql_tmpl = if use_index_friendly_where {
        OUTGOING_SQL_INDEX_FRIENDLY
    } else {
        OUTGOING_SQL_ORIGINAL
    };
    let sql = sql_tmpl.replace("{LIMIT}", &limit.to_string());

    let mut stmt = db.prepare_maybe_cached(&sql, true)?;
    let mut rows = stmt.query([])?;
    let mut count = 0usize;
    while let Some(_row) = rows.next()? {
        count += 1;
    }
    Ok(count)
}

pub fn bench_outgoing_candidates(c: &mut Criterion) {
    // Create two independent DBs so index state changes do not leak between benches.
    let db_no_index = TestDb::new();
    seed_outgoing_flags(&db_no_index.db).unwrap();
    drop_outgoing_partial_index(&db_no_index.db).unwrap();
    explain(
        "Original query".to_string(),
        &db_no_index.db,
        OUTGOING_SQL_ORIGINAL,
        200,
    );

    let db_with_index = TestDb::new();
    seed_outgoing_flags(&db_with_index.db).unwrap();
    create_outgoing_partial_index(&db_with_index.db).unwrap();
    explain(
        "Index friendly query".to_string(),
        &db_with_index.db,
        OUTGOING_SQL_INDEX_FRIENDLY,
        200,
    );

    // Bench: no index, original predicate
    {
        let test_db = db_no_index.clone();
        c.bench_function(
            "outgoing_candidates: original WHERE, NO partial index",
            move |b| {
                let db = &test_db.db;
                b.iter(|| run_outgoing_query(db, /*use_index_friendly_where=*/ false, 200).unwrap())
            },
        );
    }

    // Bench: partial index + index-friendly predicate
    {
        let test_db = db_with_index.clone();
        c.bench_function(
            "outgoing_candidates: index-friendly WHERE + partial index",
            move |b| {
                let db = &test_db.db;
                b.iter(|| run_outgoing_query(db, /*use_index_friendly_where=*/ true, 200).unwrap())
            },
        );
    }

    // also measure “partial index + original WHERE”.
    {
        let test_db = db_with_index.clone();
        c.bench_function(
            "outgoing_candidates: original WHERE + partial index",
            move |b| {
                let db = &test_db.db;
                b.iter(|| run_outgoing_query(db, /*use_index_friendly_where=*/ false, 200).unwrap())
            },
        );
    }
}

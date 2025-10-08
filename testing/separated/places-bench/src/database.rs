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
// A helper function that benches can use to validate what queries are doing
fn explain(name: &str, db: &PlacesDb, sql_tmpl: &str, subs: &[(&str, String)]) {
    let mut sql = sql_tmpl.to_string();
    for (k, v) in subs {
        sql = sql.replace(k, v);
    }
    let mut stmt = db.prepare(&format!("EXPLAIN QUERY PLAN {}", sql)).unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,    // id
                r.get::<_, i64>(1)?,    // parent
                r.get::<_, i64>(2)?,    // notused
                r.get::<_, String>(3)?, // detail
            ))
        })
        .unwrap();
    println!("--- EXPLAIN QUERY PLAN {} ---", name);
    for row in rows {
        let (id, parent, _notused, detail) = row.unwrap();
        println!("  {}-{}: {}", id, parent, detail);
    }
    println!("--- END EXPLAIN ---\n");
}
// ----------------------------------------------------------------
// fetch_outgoing benchmark, speeding up by adding a partial index
// ----------------------------------------------------------------

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
fn seed_db_for_outgoing(db: &PlacesDb) -> places::Result<()> {
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
    const LIMIT: usize = 200;
    // Create two independent DBs so index state changes do not leak between benches.
    let db_no_index = TestDb::new();
    seed_db_for_outgoing(&db_no_index.db).unwrap();
    drop_outgoing_partial_index(&db_no_index.db).unwrap();
    explain(
        "Original query",
        &db_no_index.db,
        OUTGOING_SQL_ORIGINAL,
        &[("{LIMIT}", LIMIT.to_string())],
    );

    let db_with_index = TestDb::new();
    seed_db_for_outgoing(&db_with_index.db).unwrap();
    create_outgoing_partial_index(&db_with_index.db).unwrap();
    explain(
        "Index friendly query",
        &db_with_index.db,
        OUTGOING_SQL_INDEX_FRIENDLY,
        &[("{LIMIT}", LIMIT.to_string())],
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

// -----------------------------------------------------------------------------------------
// get_top_frecent_site_infos benchmark, optimizing the where part via join/index-friendly
// -----------------------------------------------------------------------------------------

const TOP_FRECENT_ORIGINAL: &str = r#"
    SELECT h.frecency, h.title, h.url
    FROM moz_places h
    WHERE EXISTS (
        SELECT v.visit_type
        FROM moz_historyvisits v
        WHERE h.id = v.place_id
          AND (SUBSTR(h.url, 1, 6) == 'https:' OR SUBSTR(h.url, 1, 5) == 'http:')
          AND (h.last_visit_date_local + h.last_visit_date_remote) != 0
          AND ((1 << v.visit_type) & {ALLOWED_TYPES}) != 0
          AND h.frecency >= {FRECENCY} AND
          NOT h.hidden
    )
    ORDER BY h.frecency DESC
    LIMIT {LIMIT}
"#;

const TOP_FRECENT_INDEX_FRIENDLY: &str = r#"
    SELECT h.frecency, h.title, h.url
    FROM moz_places h
    WHERE h.hidden = 0
      AND (h.last_visit_date_local + h.last_visit_date_remote) != 0
      AND (h.url LIKE 'http:%' OR h.url LIKE 'https:%')
      AND h.frecency >= {FRECENCY}
      AND EXISTS (
        SELECT 1
        FROM moz_historyvisits v
        WHERE v.place_id = h.id
          AND ((1 << v.visit_type) & {ALLOWED_TYPES}) != 0
        LIMIT 1
      )
    ORDER BY h.frecency DESC, h.id DESC
    LIMIT {LIMIT}
"#;

// Seed database with test data for top frecent benchmarks
fn seed_db_for_top_frecent(db: &PlacesDb) -> places::Result<()> {
    // Ensure a mix of visible/hidden and non-zero visit dates.
    db.execute(
        r#"
        UPDATE moz_places
        SET hidden = CASE WHEN (id % 7) = 0 THEN 1 ELSE 0 END,
            last_visit_date_local = CASE WHEN (id % 3) = 0 THEN (id * 10) ELSE last_visit_date_local END,
            last_visit_date_remote = CASE WHEN (id % 5) = 0 THEN (id * 7) ELSE last_visit_date_remote END
        "#,
        [],
    )?;

    // Give many rows http/https prefixes
    db.execute(
        r#"
        UPDATE moz_places
        SET url = CASE
            WHEN (id % 2) = 0 AND url NOT LIKE 'http:%' AND url NOT LIKE 'https:%'
                 THEN 'https://example.com/item/' || id
            WHEN (id % 2) = 1 AND url NOT LIKE 'http:%' AND url NOT LIKE 'https:%'
                 THEN 'http://example.org/page/' || id
            ELSE url
        END
        "#,
        [],
    )?;

    // Ensure there are http/https origins to join against and wire origin_id.
    db.execute_batch(
        r#"
        INSERT OR IGNORE INTO moz_origins (host, rev_host, frecency, prefix)
        VALUES ('example.com', 'moc.elpmaxe.', 0, 'https');
        INSERT OR IGNORE INTO moz_origins (host, rev_host, frecency, prefix)
        VALUES ('example.org', 'gro.elpmaxe.', 0, 'http');
        "#,
    )?;
    db.execute(
        r#"
        UPDATE moz_places
        SET origin_id = (SELECT id FROM moz_origins WHERE prefix='https' LIMIT 1)
        WHERE origin_id IS NULL AND (id % 2) = 0
        "#,
        [],
    )?;
    db.execute(
        r#"
        UPDATE moz_places
        SET origin_id = (SELECT id FROM moz_origins WHERE prefix='http' LIMIT 1)
        WHERE origin_id IS NULL AND (id % 2) = 1
        "#,
        [],
    )?;

    // Keep frecency non-negative (bench predicate uses >= 0).
    db.execute(
        r#"
        UPDATE moz_places
        SET frecency = CASE
            WHEN frecency < 0 THEN ABS(frecency)
            ELSE frecency
        END
        "#,
        [],
    )?;

    db.execute_batch("ANALYZE; PRAGMA optimize;")?;
    Ok(())
}

fn create_top_frecent_cover_index(db: &PlacesDb) -> places::Result<()> {
    db.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS top_frecent_cover_idx
        ON moz_places(frecency DESC, id DESC)
        WHERE hidden = 0
        AND (last_visit_date_local + last_visit_date_remote) != 0
        AND (url GLOB 'http:*' OR url GLOB 'https:*');

        CREATE INDEX IF NOT EXISTS idx_visits_place_type
        ON moz_historyvisits(place_id, visit_type);
        "#,
    )?;
    db.execute_batch("ANALYZE; PRAGMA optimize;")?;
    Ok(())
}

fn drop_top_frecent_cover_index(db: &PlacesDb) -> places::Result<()> {
    db.execute_batch("DROP INDEX IF EXISTS top_frecent_cover_idx;")?;
    db.execute_batch("DROP INDEX IF EXISTS idx_visits_place_type;")?;
    db.execute_batch("ANALYZE; PRAGMA optimize;")?;
    Ok(())
}

fn run_top_frecent_query(
    db: &PlacesDb,
    sql_tmpl: &str,
    limit: usize,
    frecency_threshold: i64,
    allowed_types_mask: i64,
) -> places::Result<usize> {
    let subs = &[
        ("{LIMIT}", limit.to_string()),
        ("{FRECENCY}", frecency_threshold.to_string()),
        ("{ALLOWED_TYPES}", allowed_types_mask.to_string()),
    ];
    let mut sql = sql_tmpl.to_string();
    for (k, v) in subs {
        sql = sql.replace(k, v);
    }

    let mut stmt = db.prepare_maybe_cached(&sql, true)?;
    let mut rows = stmt.query([])?;
    let mut count = 0usize;
    while let Some(_row) = rows.next()? {
        count += 1;
    }
    Ok(count)
}

pub fn bench_top_frecent(c: &mut Criterion) {
    const LIMIT: usize = 200;
    const FRECENCY_THRESHOLD: i64 = 0;
    // Allow several visit types
    const ALLOWED_TYPES: i64 = (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5) | (1 << 6);

    let db_no_index = TestDb::new();
    seed_db_for_top_frecent(&db_no_index.db).unwrap();
    drop_top_frecent_cover_index(&db_no_index.db).unwrap();

    let db_with_index = TestDb::new();
    seed_db_for_top_frecent(&db_with_index.db).unwrap();
    create_top_frecent_cover_index(&db_with_index.db).unwrap();
    explain(
        "Index friendly query",
        &db_with_index.db,
        TOP_FRECENT_INDEX_FRIENDLY,
        &[
            ("{LIMIT}", LIMIT.to_string()),
            ("{FRECENCY}", FRECENCY_THRESHOLD.to_string()),
            ("{ALLOWED_TYPES}", ALLOWED_TYPES.to_string()),
        ],
    );

    // 1) Original query shape
    {
        let tdb = db_no_index.clone();
        c.bench_function("top_frecent: ORIGINAL WHERE", move |b| {
            let db = &tdb.db;
            b.iter(|| {
                run_top_frecent_query(
                    db,
                    TOP_FRECENT_ORIGINAL,
                    LIMIT,
                    FRECENCY_THRESHOLD,
                    ALLOWED_TYPES,
                )
                .unwrap()
            })
        });
    }

    {
        let tdb = db_with_index.clone();
        c.bench_function("top_frecent: optimized_1", move |b| {
            let db = &tdb.db;
            b.iter(|| {
                run_top_frecent_query(
                    db,
                    TOP_FRECENT_INDEX_FRIENDLY,
                    LIMIT,
                    FRECENCY_THRESHOLD,
                    ALLOWED_TYPES,
                )
                .unwrap()
            })
        });
    }
}

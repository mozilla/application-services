/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use places::{api::places_api::PlacesApi, types::VisitTransition, ErrorKind, Result, Timestamp};
use rusqlite::{Connection, NO_PARAMS};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use sync_guid::Guid;
use tempfile::tempdir;

fn empty_fennec_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(include_str!("./fennec_history_schema.sql"))?;
    Ok(conn)
}

#[derive(Clone, Debug)]
struct FennecHistory {
    title: Option<String>,
    url: String,
    visits: u16,
    visits_local: u16,
    visits_remote: u16,
    date: Timestamp,
    date_local: Timestamp,
    date_remote: Timestamp,
    created: Timestamp,
    modified: Timestamp,
    guid: Guid,
    deleted: bool,
}

impl FennecHistory {
    fn insert_into_db(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare(&
            "INSERT OR IGNORE INTO history(title, url, visits, visits_local, visits_remote, date,
                                           date_local, date_remote, created, modified, guid, deleted)
             VALUES (:title, :url, :visits, :visits_local, :visits_remote, :date,
                     :date_local, :date_remote, :created, :modified, :guid, :deleted)"
        )?;
        stmt.execute_named(rusqlite::named_params! {
            ":title": self.title,
            ":url": self.url,
            ":visits": self.visits,
            ":visits_local": self.visits_local,
            ":visits_remote": self.visits_remote,
            ":date": self.date,
            ":date_local": self.date_local,
            ":date_remote": self.date_remote,
            ":created": self.created,
            ":modified": self.modified,
            ":guid": self.guid,
            ":deleted": self.deleted,
        })?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct FennecVisit<'a> {
    history: &'a FennecHistory,
    visit_type: VisitTransition,
    date: Timestamp,
    is_local: bool,
}

impl<'a> FennecVisit<'a> {
    fn insert_into_db(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare(
            &"INSERT OR IGNORE INTO visits(history_guid, visit_type, date, is_local)
             VALUES (:history_guid, :visit_type, :date, :is_local)",
        )?;
        stmt.execute_named(rusqlite::named_params! {
            ":history_guid": self.history.guid,
            ":visit_type": self.visit_type,
            ":date": self.date,
            ":is_local": self.is_local,
        })?;
        Ok(())
    }
}

static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

// Helps debugging to use these instead of actually random ones.
fn next_guid() -> Guid {
    let c = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let v = format!("test{}_______", c);
    let s = &v[..12];
    Guid::from(s)
}

impl Default for FennecHistory {
    fn default() -> Self {
        Self {
            title: None,
            url: String::default(),
            visits: 0,
            visits_local: 0,
            visits_remote: 0,
            date: Timestamp::now(),
            date_local: Timestamp::now(),
            date_remote: Timestamp::now(),
            created: Timestamp::now(),
            modified: Timestamp::now(),
            guid: next_guid(),
            deleted: false,
        }
    }
}

fn insert_history_and_visits(
    conn: &Connection,
    history: &[FennecHistory],
    visits: &[FennecVisit],
) -> Result<()> {
    for h in history {
        h.insert_into_db(conn)?;
    }
    for v in visits {
        v.insert_into_db(conn)?;
    }
    Ok(())
}

#[test]
fn test_import_unsupported_db_version() -> Result<()> {
    let tmpdir = tempdir().unwrap();
    let fennec_path = tmpdir.path().join("browser.db");
    let fennec_db = empty_fennec_db(&fennec_path)?;
    fennec_db.execute("PRAGMA user_version=99", NO_PARAMS)?;
    let places_api = PlacesApi::new(tmpdir.path().join("places.sqlite"))?;
    match places::import::import_fennec_history(&places_api, fennec_path)
        .unwrap_err()
        .kind()
    {
        ErrorKind::UnsupportedDatabaseVersion(_) => {}
        _ => unreachable!("Should fail with UnsupportedDatabaseVersion!"),
    }
    Ok(())
}

#[test]
fn test_import() -> Result<()> {
    use places::db::PlacesDb;
    use places::storage::fetch_page_info;
    use url::Url;

    fn check_visit_counts(
        db: &PlacesDb,
        url: &str,
        expected_local: i32,
        expected_remote: i32,
    ) -> Result<()> {
        let pi = fetch_page_info(db, &Url::parse(url)?)?.expect("has page");
        assert_eq!(pi.page.visit_count_local, expected_local);
        assert_eq!(pi.page.visit_count_remote, expected_remote);
        Ok(())
    }

    let tmpdir = tempdir().unwrap();
    let fennec_path = tmpdir.path().join("browser.db");
    let fennec_db = empty_fennec_db(&fennec_path)?;

    let history = [
        FennecHistory {
            title: Some("Welcome to bobo.com".to_owned()),
            url: "https://bobo.com/".to_owned(),
            ..Default::default()
        },
        FennecHistory {
            title: Some("Mozilla.org".to_owned()),
            url: "https://mozilla.org/".to_owned(),
            ..Default::default()
        },
        FennecHistory {
            url: "https://foo.bar/".to_owned(),
            ..Default::default()
        },
        FennecHistory {
            url: "https://gonnacolide.guid".to_owned(),
            guid: Guid::from("colidingguid"), // This GUID already exists in the DB, but with a different URL.
            ..Default::default()
        },
        FennecHistory {
            url: "https://existing.guid".to_owned(), // This GUID already exists in the DB, with the same URL.
            guid: Guid::from("existingguid"),
            ..Default::default()
        },
        FennecHistory {
            url: "https://existing.url".to_owned(), // This URL already exists in the DB.
            ..Default::default()
        },
        FennecHistory {
            url: "I'm a super invalid URL, yo".to_owned(),
            ..Default::default()
        },
        // Add "http://💖.com/💖" using an escaped string.
        FennecHistory {
            url: "http://\u{1F496}.com/\u{1F496}".to_owned(),
            ..Default::default()
        },
        // Add "http://😍.com/😍" already punycoded.
        FennecHistory {
            url: "http://xn--r28h.com/%F0%9F%98%8D".to_owned(),
            ..Default::default()
        },
    ];
    let visits = [
        FennecVisit {
            history: &history[0],
            visit_type: VisitTransition::Typed,
            date: Timestamp::from(1_565_117_389_897),
            is_local: true,
        },
        FennecVisit {
            history: &history[0],
            visit_type: VisitTransition::Link,
            date: Timestamp::from(1_565_117_389_898),
            is_local: false,
        },
        FennecVisit {
            history: &history[1],
            visit_type: VisitTransition::Link,
            date: Timestamp::from(1), // Invalid timestamp should get corrected!
            is_local: false,
        },
        FennecVisit {
            history: &history[1],
            visit_type: VisitTransition::Link,
            date: Timestamp::from(1_565_117_123_123_123), // Microsecond timestamp should be imported.
            is_local: false,
        },
        FennecVisit {
            history: &history[3],
            visit_type: VisitTransition::Link,
            date: Timestamp::from(1_565_117_389_898),
            is_local: true,
        },
        FennecVisit {
            history: &history[4],
            visit_type: VisitTransition::Link,
            date: Timestamp::from(1_565_117_389_898),
            is_local: true,
        },
        FennecVisit {
            history: &history[5],
            visit_type: VisitTransition::Link,
            date: Timestamp::from(1_565_117_389_898),
            is_local: true,
        },
        FennecVisit {
            history: &history[7],
            visit_type: VisitTransition::Link,
            date: Timestamp::from(1_565_117_389_898),
            is_local: true,
        },
        FennecVisit {
            history: &history[8],
            visit_type: VisitTransition::Link,
            date: Timestamp::from(1_565_117_389_898),
            is_local: false,
        },
    ];
    insert_history_and_visits(&fennec_db, &history, &visits)?;

    let places_api = PlacesApi::new(tmpdir.path().join("places.sqlite"))?;

    // Insert some places with GUIDs that colide with the imported data.
    let conn = places_api.open_connection(places::ConnectionType::ReadWrite)?;
    conn.execute(
        "INSERT INTO moz_places (guid, url, url_hash)
        VALUES ('colidingguid', 'https://coliding.guid', hash('https://coliding.guid'))",
        NO_PARAMS,
    )
    .expect("should insert");
    conn.execute(
        "INSERT INTO moz_places (guid, url, url_hash)
        VALUES ('existingguid', 'https://existing.guid', hash('https://existing.guid'))",
        NO_PARAMS,
    )
    .expect("should insert");
    conn.execute(
        "INSERT INTO moz_places (guid, url, url_hash)
        VALUES ('boboguid1', 'https://existing.url', hash('https://existing.url'))",
        NO_PARAMS,
    )
    .expect("should insert");

    places::import::import_fennec_history(&places_api, fennec_path)?;

    // Check we imported things correctly.
    check_visit_counts(&conn, "https://bobo.com/", 1, 1)?;
    check_visit_counts(&conn, "https://mozilla.org/", 0, 2)?;
    // foo.bar has no visits, but should still get a place.
    check_visit_counts(&conn, "https://foo.bar/", 0, 0)?;
    check_visit_counts(&conn, "https://gonnacolide.guid/", 1, 0)?;
    check_visit_counts(&conn, "https://existing.guid", 1, 0)?;
    check_visit_counts(&conn, "https://existing.guid", 1, 0)?;
    check_visit_counts(&conn, "http://💖.com/💖", 1, 0)?;
    check_visit_counts(&conn, "http://😍.com/😍", 0, 1)?;

    // Uncomment the following to debug with cargo test -- --nocapture.
    // println!(
    //     "Places DB Path: {}",
    //     tmpdir.path().join("places.sqlite").to_str().unwrap()
    // );
    // ::std::process::exit(0);

    Ok(())
}

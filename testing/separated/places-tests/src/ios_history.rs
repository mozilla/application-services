/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use places::{
    api::places_api::{ConnectionType, PlacesApi},
    apply_observation,
    storage::history::{self, get_visit_infos},
    Result, VisitObservation, VisitTransition, VisitTransitionSet,
};
use rusqlite::Connection;
use std::path::Path;
use std::{str::FromStr, time::Duration};
use tempfile::tempdir;
use types::Timestamp;
use url::Url;

fn empty_ios_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(include_str!("./ios_schema.sql"))?;
    Ok(conn)
}

#[derive(Default, Debug)]
struct IOSHistory {
    id: u64,
    guid: String,
    url: Option<String>,
    title: String,
    is_deleted: bool,
    should_upload: bool,
}

#[derive(Debug, Default, Clone)]

struct IOSVisit {
    id: u64,
    site_id: u64,
    date: i64,
    type_: u64,
    is_local: bool,
}

#[derive(Debug, Default)]
struct HistoryTable(Vec<IOSHistory>);

#[derive(Debug, Default)]
struct VisitTable(Vec<IOSVisit>);

impl HistoryTable {
    fn populate(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare(
            "INSERT INTO history(
                id,
                guid,
                url,
                title,
                is_deleted,
                should_upload
            ) VALUES (
                :id,
                :guid,
                :url,
                :title,
                :is_deleted,
                :should_upload
            )",
        )?;
        for history_item in &self.0 {
            stmt.execute(rusqlite::named_params! {
                ":guid": history_item.id,
                ":guid": history_item.guid,
                ":url": history_item.url,
                ":title": history_item.title,
                ":is_deleted": history_item.is_deleted,
                ":should_upload": history_item.should_upload,
            })?;
        }
        Ok(())
    }
}

impl VisitTable {
    fn populate(&self, conn: &Connection) -> Result<()> {
        let mut stmt = conn.prepare(
            "INSERT INTO visits (
                id,
                siteID,
                date,
                type,
                is_local
            ) VALUES (
                :id,
                :siteID,
                :date,
                :type,
                :is_local
            )",
        )?;
        for visit in &self.0 {
            stmt.execute(rusqlite::named_params! {
                ":id": visit.id,
                ":siteID": visit.site_id,
                ":date": visit.date,
                ":type": visit.type_,
                ":is_local": visit.is_local,
            })?;
        }
        Ok(())
    }
}

#[test]
fn test_import_empty() -> Result<()> {
    let tmpdir = tempdir().unwrap();
    let history = HistoryTable::default();
    let visits = VisitTable::default();
    let ios_path = tmpdir.path().join("browser.db");
    let ios_db = empty_ios_db(&ios_path)?;

    history.populate(&ios_db)?;
    visits.populate(&ios_db)?;
    let places_api = PlacesApi::new(tmpdir.path().join("places.sqlite"))?;
    let conn = places_api.open_connection(ConnectionType::ReadWrite)?;
    places::import::import_ios_history(&conn, ios_path, 0)?;

    Ok(())
}

#[test]
fn test_import_basic() -> Result<()> {
    let tmpdir = tempdir().unwrap();
    let ios_path = tmpdir.path().join("browser.db");
    let ios_db = empty_ios_db(&ios_path)?;
    let history_entry = IOSHistory {
        id: 1,
        guid: "EXAMPLE GUID".to_string(),
        url: Some("https://example.com".to_string()),
        title: "Example(dot)com".to_string(),
        is_deleted: false,
        should_upload: false,
    };

    // We subtract a bit because our sanitization logic is smart and rejects
    // visits that have a future timestamp,
    let before_first_visit_ts = Timestamp::now()
        .checked_sub(Duration::from_secs(10000))
        .unwrap();
    let first_visit_ts = before_first_visit_ts
        .checked_add(Duration::from_secs(100))
        .unwrap();
    let visit = IOSVisit {
        id: 1,
        site_id: 1,
        // Dates in iOS are represented as μs
        // we make sure that they get converted properly.
        // when we compare them later we will compare against
        // milliseconds
        date: first_visit_ts.as_millis_i64() * 1000,
        type_: 1,
        ..Default::default()
    };

    let second_visit_ts = first_visit_ts
        .checked_add(Duration::from_secs(100))
        .unwrap();

    let other_visit = IOSVisit {
        id: 2,
        site_id: 1,
        // Dates in iOS are represented as μs
        date: second_visit_ts.as_millis_i64() * 1000,
        type_: 1,
        ..Default::default()
    };

    let history_table = HistoryTable(vec![history_entry]);
    let visit_table = VisitTable(vec![visit, other_visit]);
    history_table.populate(&ios_db)?;
    visit_table.populate(&ios_db)?;

    let places_api = PlacesApi::new(tmpdir.path().join("places.sqlite"))?;
    let conn = places_api.open_connection(ConnectionType::ReadWrite)?;
    places::import::import_ios_history(&conn, ios_path, 0)?;

    let places_db = places_api.open_connection(ConnectionType::ReadOnly)?;
    let visit_count = history::get_visit_count(&places_db, VisitTransitionSet::empty())?;
    assert_eq!(visit_count, 2);
    let url = Url::from_str("https://example.com").unwrap();
    let visited = history::get_visited(&places_db, vec![url]).unwrap();
    assert!(visited[0]);
    let visit_infos = history::get_visit_infos(
        &places_db,
        before_first_visit_ts,
        Timestamp::now(),
        VisitTransitionSet::empty(),
    )?;
    assert_eq!(visit_infos[0].title, Some("Example(dot)com".to_owned()));
    assert_eq!(visit_infos[1].title, Some("Example(dot)com".to_owned()));
    assert_eq!(visit_infos[0].timestamp, first_visit_ts);
    assert_eq!(visit_infos[1].timestamp, second_visit_ts);
    Ok(())
}

#[test]
fn test_update_missing_title() -> Result<()> {
    let tmpdir = tempdir().unwrap();

    let places_api = PlacesApi::new(tmpdir.path().join("places.sqlite"))?;
    let mut conn = places_api.open_connection(ConnectionType::ReadWrite)?;
    apply_observation(
        &mut conn,
        VisitObservation::new(Url::parse("https://example.com/").unwrap())
            .with_visit_type(Some(VisitTransition::Link)),
    )?;
    apply_observation(
        &mut conn,
        VisitObservation::new(Url::parse("https://mozilla.org/").unwrap())
            .with_title(Some("Mozilla!".to_string()))
            .with_visit_type(Some(VisitTransition::Link)),
    )?;
    let visit_infos = get_visit_infos(
        &conn,
        Timestamp::EARLIEST,
        Timestamp::now(),
        VisitTransitionSet::empty(),
    )?;
    assert_eq!(visit_infos.len(), 2);
    // We verify that before the migration example.com had no title
    // and mozilla.org had "Mozilla!" as title
    for visit in visit_infos {
        if visit.url.to_string() == "https://example.com/" {
            assert!(visit.title.is_none())
        } else if visit.url.to_string() == "https://mozilla.org/" {
            assert_eq!(visit.title, Some("Mozilla!".to_string()))
        } else {
            panic!("Unexpected visit: {}", visit.url)
        }
    }

    let ios_path = tmpdir.path().join("browser.db");
    let ios_db = empty_ios_db(&ios_path)?;
    let example_com_entry = IOSHistory {
        id: 1,
        guid: "EXAMPLE GUID".to_string(),
        url: Some("https://example.com/".to_string()),
        title: "Example(dot)com".to_string(),
        is_deleted: false,
        should_upload: false,
    };

    let mozilla_org_entry = IOSHistory {
        id: 1,
        guid: "EXAMPLE GUID2".to_string(),
        url: Some("https://mozilla.org/".to_string()),
        title: "New Mozilla Title".to_string(),
        is_deleted: false,
        should_upload: false,
    };

    let history_table = HistoryTable(vec![example_com_entry, mozilla_org_entry]);
    history_table.populate(&ios_db)?;

    // We now run the migration, both places should get an updated title
    places::import::import_ios_history(&conn, ios_path, 0)?;
    let visit_infos = get_visit_infos(
        &conn,
        Timestamp::EARLIEST,
        Timestamp::now(),
        VisitTransitionSet::empty(),
    )?;
    assert_eq!(visit_infos.len(), 2);

    for visit in visit_infos {
        if visit.url.to_string() == "https://example.com/" {
            assert_eq!(visit.title, Some("Example(dot)com".to_string()))
        } else if visit.url.to_string() == "https://mozilla.org/" {
            assert_eq!(visit.title, Some("New Mozilla Title".to_string()))
        } else {
            panic!("Unexpected visit: {}", visit.url)
        }
    }

    Ok(())
}

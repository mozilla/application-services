/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// A "storage" module - this module is intended to be the layer between the
// API and the database.

pub mod bookmarks;
pub mod history;
pub mod history_metadata;
pub mod tags;

use crate::db::PlacesDb;
use crate::error::{warn, Error, InvalidPlaceInfo, Result};
use crate::ffi::HistoryVisitInfo;
use crate::ffi::TopFrecentSiteInfo;
use crate::frecency::{calculate_frecency, DEFAULT_FRECENCY_SETTINGS};
use crate::types::{SyncStatus, UnknownFields, VisitType};
use interrupt_support::SqlInterruptScope;
use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::Result as RusqliteResult;
use rusqlite::{Connection, Row};
use serde_derive::*;
use sql_support::{self, ConnExt};
use std::fmt;
use sync_guid::Guid as SyncGuid;
use types::Timestamp;
use url::Url;

/// From https://searchfox.org/mozilla-central/rev/93905b660f/toolkit/components/places/PlacesUtils.jsm#189
pub const URL_LENGTH_MAX: usize = 65536;
pub const TITLE_LENGTH_MAX: usize = 4096;
pub const TAG_LENGTH_MAX: usize = 100;
// pub const DESCRIPTION_LENGTH_MAX: usize = 256;

// Typesafe way to manage RowIds. Does it make sense? A better way?
#[derive(
    Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Deserialize, Serialize, Default, Hash,
)]
pub struct RowId(pub i64);

impl From<RowId> for i64 {
    // XXX - ToSql!
    #[inline]
    fn from(id: RowId) -> Self {
        id.0
    }
}

impl fmt::Display for RowId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ToSql for RowId {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0))
    }
}

impl FromSql for RowId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value.as_i64().map(RowId)
    }
}

#[derive(Debug)]
pub struct PageInfo {
    pub url: Url,
    pub guid: SyncGuid,
    pub row_id: RowId,
    pub title: String,
    pub hidden: bool,
    pub preview_image_url: Option<Url>,
    pub typed: u32,
    pub frecency: i32,
    pub visit_count_local: i32,
    pub visit_count_remote: i32,
    pub last_visit_date_local: Timestamp,
    pub last_visit_date_remote: Timestamp,
    pub sync_status: SyncStatus,
    pub sync_change_counter: u32,
    pub unknown_fields: UnknownFields,
}

impl PageInfo {
    pub fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            url: Url::parse(&row.get::<_, String>("url")?)?,
            guid: row.get::<_, String>("guid")?.into(),
            row_id: row.get("id")?,
            title: row.get::<_, Option<String>>("title")?.unwrap_or_default(),
            hidden: row.get("hidden")?,
            preview_image_url: match row.get::<_, Option<String>>("preview_image_url")? {
                Some(ref preview_image_url) => Some(Url::parse(preview_image_url)?),
                None => None,
            },
            typed: row.get("typed")?,

            frecency: row.get("frecency")?,
            visit_count_local: row.get("visit_count_local")?,
            visit_count_remote: row.get("visit_count_remote")?,

            last_visit_date_local: row
                .get::<_, Option<Timestamp>>("last_visit_date_local")?
                .unwrap_or_default(),
            last_visit_date_remote: row
                .get::<_, Option<Timestamp>>("last_visit_date_remote")?
                .unwrap_or_default(),

            sync_status: SyncStatus::from_u8(row.get::<_, u8>("sync_status")?),
            sync_change_counter: row
                .get::<_, Option<u32>>("sync_change_counter")?
                .unwrap_or_default(),
            unknown_fields: match row.get::<_, Option<String>>("unknown_fields")? {
                Some(v) => serde_json::from_str(&v)?,
                None => UnknownFields::new(),
            },
        })
    }
}

// fetch_page_info gives you one of these.
#[derive(Debug)]
pub struct FetchedPageInfo {
    pub page: PageInfo,
    // XXX - not clear what this is used for yet, and whether it should be local, remote or either?
    // The sql below isn't quite sure either :)
    pub last_visit_id: Option<RowId>,
}

impl FetchedPageInfo {
    pub fn from_row(row: &Row<'_>) -> Result<Self> {
        Ok(Self {
            page: PageInfo::from_row(row)?,
            last_visit_id: row.get::<_, Option<RowId>>("last_visit_id")?,
        })
    }
}

// History::FetchPageInfo
pub fn fetch_page_info(db: &PlacesDb, url: &Url) -> Result<Option<FetchedPageInfo>> {
    let sql = "
      SELECT guid, url, id, title, hidden, typed, frecency,
             visit_count_local, visit_count_remote,
             last_visit_date_local, last_visit_date_remote,
             sync_status, sync_change_counter, preview_image_url,
             unknown_fields,
             (SELECT id FROM moz_historyvisits
              WHERE place_id = h.id
                AND (visit_date = h.last_visit_date_local OR
                     visit_date = h.last_visit_date_remote)) AS last_visit_id
      FROM moz_places h
      WHERE url_hash = hash(:page_url) AND url = :page_url";
    db.try_query_row(
        sql,
        &[(":page_url", &String::from(url.clone()))],
        FetchedPageInfo::from_row,
        true,
    )
}

fn new_page_info(db: &PlacesDb, url: &Url, new_guid: Option<SyncGuid>) -> Result<PageInfo> {
    let guid = match new_guid {
        Some(guid) => guid,
        None => SyncGuid::random(),
    };
    let url_str = url.as_str();
    if url_str.len() > URL_LENGTH_MAX {
        // Generally callers check this first (bookmarks don't, history does).
        return Err(Error::InvalidPlaceInfo(InvalidPlaceInfo::UrlTooLong));
    }
    let sql = "INSERT INTO moz_places (guid, url, url_hash)
               VALUES (:guid, :url, hash(:url))";
    db.execute_cached(sql, &[(":guid", &guid as &dyn ToSql), (":url", &url_str)])?;
    Ok(PageInfo {
        url: url.clone(),
        guid,
        row_id: RowId(db.conn().last_insert_rowid()),
        title: "".into(),
        hidden: true, // will be set to false as soon as a non-hidden visit appears.
        preview_image_url: None,
        typed: 0,
        frecency: -1,
        visit_count_local: 0,
        visit_count_remote: 0,
        last_visit_date_local: Timestamp(0),
        last_visit_date_remote: Timestamp(0),
        sync_status: SyncStatus::New,
        sync_change_counter: 0,
        unknown_fields: UnknownFields::new(),
    })
}

impl HistoryVisitInfo {
    fn from_row(row: &rusqlite::Row<'_>) -> Result<Self> {
        let visit_type = VisitType::from_primitive(row.get::<_, u8>("visit_type")?)
            // Do we have an existing error we use for this? For now they
            // probably don't care too much about VisitType, so this
            // is fine.
            .unwrap_or(VisitType::Link);
        let visit_date: Timestamp = row.get("visit_date")?;
        let url: String = row.get("url")?;
        let preview_image_url: Option<String> = row.get("preview_image_url")?;
        Ok(Self {
            url: Url::parse(&url)?,
            title: row.get("title")?,
            timestamp: visit_date,
            visit_type,
            is_hidden: row.get("hidden")?,
            preview_image_url: match preview_image_url {
                Some(s) => Some(Url::parse(&s)?),
                None => None,
            },
            is_remote: !row.get("is_local")?,
        })
    }
}

impl TopFrecentSiteInfo {
    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> Result<Self> {
        let url: String = row.get("url")?;
        Ok(Self {
            url: Url::parse(&url)?,
            title: row.get("title")?,
        })
    }
}

#[derive(Debug)]
pub struct RunMaintenanceMetrics {
    pub pruned_visits: bool,
    pub db_size_before: u32,
    pub db_size_after: u32,
}

/// Run maintenance on the places DB (prune step)
///
/// The `run_maintenance_*()` functions are intended to be run during idle time and will take steps
/// to clean up / shrink the database.  They're split up so that we can time each one in the
/// Kotlin wrapper code (This is needed because we only have access to the Glean API in Kotlin and
/// it supports a stop-watch style API, not recording specific values).
///
/// db_size_limit is the approximate storage limit in bytes.  If the database is using more space
/// than this, some older visits will be deleted to free up space.  Pass in a 0 to skip this.
///
/// prune_limit is the maximum number of visits to prune if the database is over db_size_limit
pub fn run_maintenance_prune(
    conn: &PlacesDb,
    db_size_limit: u32,
    prune_limit: u32,
) -> Result<RunMaintenanceMetrics> {
    let db_size_before = conn.get_db_size()?;
    let should_prune = db_size_limit > 0 && db_size_before > db_size_limit;
    if should_prune {
        history::prune_older_visits(conn, prune_limit)?;
    }
    let db_size_after = conn.get_db_size()?;
    Ok(RunMaintenanceMetrics {
        pruned_visits: should_prune,
        db_size_before,
        db_size_after,
    })
}

/// Run maintenance on the places DB (vacuum step)
///
/// The `run_maintenance_*()` functions are intended to be run during idle time and will take steps
/// to clean up / shrink the database.  They're split up so that we can time each one in the
/// Kotlin wrapper code (This is needed because we only have access to the Glean API in Kotlin and
/// it supports a stop-watch style API, not recording specific values).
pub fn run_maintenance_vacuum(conn: &PlacesDb) -> Result<()> {
    let auto_vacuum_setting: u32 = conn.query_one("PRAGMA auto_vacuum")?;
    if auto_vacuum_setting == 2 {
        // Ideally, we run an incremental vacuum to delete 2 pages
        conn.execute_one("PRAGMA incremental_vacuum(2)")?;
    } else {
        // If auto_vacuum=incremental isn't set, configure it and run a full vacuum.
        warn!("run_maintenance_vacuum: Need to run a full vacuum to set auto_vacuum=incremental");
        conn.execute_one("PRAGMA auto_vacuum=incremental")?;
        conn.execute_one("VACUUM")?;
    }
    Ok(())
}

/// Run maintenance on the places DB (optimize step)
///
/// The `run_maintenance_*()` functions are intended to be run during idle time and will take steps
/// to clean up / shrink the database.  They're split up so that we can time each one in the
/// Kotlin wrapper code (This is needed because we only have access to the Glean API in Kotlin and
/// it supports a stop-watch style API, not recording specific values).
pub fn run_maintenance_optimize(conn: &PlacesDb) -> Result<()> {
    conn.execute_one("PRAGMA optimize")?;
    Ok(())
}

/// Run maintenance on the places DB (checkpoint step)
///
/// The `run_maintenance_*()` functions are intended to be run during idle time and will take steps
/// to clean up / shrink the database.  They're split up so that we can time each one in the
/// Kotlin wrapper code (This is needed because we only have access to the Glean API in Kotlin and
/// it supports a stop-watch style API, not recording specific values).
pub fn run_maintenance_checkpoint(conn: &PlacesDb) -> Result<()> {
    conn.execute_one("PRAGMA wal_checkpoint(PASSIVE)")?;
    Ok(())
}

pub fn update_all_frecencies_at_once(db: &PlacesDb, scope: &SqlInterruptScope) -> Result<()> {
    let tx = db.begin_transaction()?;

    let need_frecency_update = tx.query_rows_and_then(
        "SELECT place_id FROM moz_places_stale_frecencies",
        [],
        |r| r.get::<_, i64>(0),
    )?;
    scope.err_if_interrupted()?;
    let frecencies = need_frecency_update
        .iter()
        .map(|places_id| {
            scope.err_if_interrupted()?;
            Ok((
                *places_id,
                calculate_frecency(db, &DEFAULT_FRECENCY_SETTINGS, *places_id, Some(false))?,
            ))
        })
        .collect::<Result<Vec<(i64, i32)>>>()?;

    if frecencies.is_empty() {
        return Ok(());
    }
    // Update all frecencies in one fell swoop
    tx.execute_batch(&format!(
        "WITH frecencies(id, frecency) AS (
            VALUES {}
            )
            UPDATE moz_places SET
            frecency = (SELECT frecency FROM frecencies f
                        WHERE f.id = id)
            WHERE id IN (SELECT f.id FROM frecencies f)",
        sql_support::repeat_display(frecencies.len(), ",", |index, f| {
            let (id, frecency) = frecencies[index];
            write!(f, "({}, {})", id, frecency)
        })
    ))?;

    scope.err_if_interrupted()?;

    // ...And remove them from the stale table.
    tx.execute_batch(&format!(
        "DELETE FROM moz_places_stale_frecencies
         WHERE place_id IN ({})",
        sql_support::repeat_display(frecencies.len(), ",", |index, f| {
            let (id, _) = frecencies[index];
            write!(f, "{}", id)
        })
    ))?;
    tx.commit()?;

    Ok(())
}

pub(crate) fn put_meta(conn: &Connection, key: &str, value: &dyn ToSql) -> Result<()> {
    conn.execute_cached(
        "REPLACE INTO moz_meta (key, value) VALUES (:key, :value)",
        &[(":key", &key as &dyn ToSql), (":value", value)],
    )?;
    Ok(())
}

pub(crate) fn get_meta<T: FromSql>(db: &PlacesDb, key: &str) -> Result<Option<T>> {
    let res = db.try_query_one(
        "SELECT value FROM moz_meta WHERE key = :key",
        &[(":key", &key)],
        true,
    )?;
    Ok(res)
}

pub(crate) fn delete_meta(db: &PlacesDb, key: &str) -> Result<()> {
    db.execute_cached("DELETE FROM moz_meta WHERE key = :key", &[(":key", &key)])?;
    Ok(())
}

/// Delete all items in the temp tables we use for staging changes.
pub fn delete_pending_temp_tables(conn: &PlacesDb) -> Result<()> {
    conn.execute_batch(
        "DELETE FROM moz_updateoriginsinsert_temp;
         DELETE FROM moz_updateoriginsupdate_temp;
         DELETE FROM moz_updateoriginsdelete_temp;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::test::new_mem_connection;
    use crate::observation::VisitObservation;
    use bookmarks::{
        delete_bookmark, insert_bookmark, BookmarkPosition, BookmarkRootGuid, InsertableBookmark,
        InsertableItem,
    };
    use history::apply_observation;

    #[test]
    fn test_meta() {
        let conn = new_mem_connection();
        let value1 = "value 1".to_string();
        let value2 = "value 2".to_string();
        assert!(get_meta::<String>(&conn, "foo")
            .expect("should get")
            .is_none());
        put_meta(&conn, "foo", &value1).expect("should put");
        assert_eq!(
            get_meta(&conn, "foo").expect("should get new val"),
            Some(value1)
        );
        put_meta(&conn, "foo", &value2).expect("should put an existing value");
        assert_eq!(get_meta(&conn, "foo").expect("should get"), Some(value2));
        delete_meta(&conn, "foo").expect("should delete");
        assert!(get_meta::<String>(&conn, "foo")
            .expect("should get non-existing")
            .is_none());
        delete_meta(&conn, "foo").expect("delete non-existing should work");
    }

    // Here we try and test that we replicate desktop behaviour, which isn't that obvious.
    // * create a bookmark
    // * remove the bookmark - this doesn't remove the place or origin - probably because in
    //   real browsers there will be visits for the URL existing, but this still smells like
    //   a bug - see https://bugzilla.mozilla.org/show_bug.cgi?id=1650511#c34
    // * Arrange for history for that item to be removed, via various means
    // At this point the origin and place should be removed. The only code (in desktop and here) which
    // removes places with a foreign_count of zero is that history removal!

    #[test]
    fn test_removal_delete_visits_between() {
        do_test_removal_places_and_origins(|conn: &PlacesDb, _guid: &SyncGuid| {
            history::delete_visits_between(conn, Timestamp::EARLIEST, Timestamp::now())
        })
    }

    #[test]
    fn test_removal_delete_visits_for() {
        do_test_removal_places_and_origins(|conn: &PlacesDb, guid: &SyncGuid| {
            history::delete_visits_for(conn, guid)
        })
    }

    #[test]
    fn test_removal_prune() {
        do_test_removal_places_and_origins(|conn: &PlacesDb, _guid: &SyncGuid| {
            history::prune_older_visits(conn, 6)
        })
    }

    #[test]
    fn test_removal_visit_at_time() {
        do_test_removal_places_and_origins(|conn: &PlacesDb, _guid: &SyncGuid| {
            let url = Url::parse("http://example.com/foo").unwrap();
            let visit = Timestamp::from(727_747_200_001);
            history::delete_place_visit_at_time(conn, &url, visit)
        })
    }

    #[test]
    fn test_removal_everything() {
        do_test_removal_places_and_origins(|conn: &PlacesDb, _guid: &SyncGuid| {
            history::delete_everything(conn)
        })
    }

    // The core test - takes a function which deletes history.
    fn do_test_removal_places_and_origins<F>(removal_fn: F)
    where
        F: FnOnce(&PlacesDb, &SyncGuid) -> Result<()>,
    {
        let conn = new_mem_connection();
        let url = Url::parse("http://example.com/foo").unwrap();
        let bm = InsertableItem::Bookmark {
            b: InsertableBookmark {
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: BookmarkPosition::Append,
                date_added: None,
                last_modified: None,
                guid: None,
                url: url.clone(),
                title: Some("the title".into()),
            },
        };
        assert_eq!(
            conn.query_one::<i64>("SELECT COUNT(*) FROM moz_bookmarks;")
                .unwrap(),
            5
        ); // our 5 roots.
        let bookmark_guid = insert_bookmark(&conn, bm).unwrap();
        let place_guid = fetch_page_info(&conn, &url)
            .expect("should work")
            .expect("must exist")
            .page
            .guid;
        // the place should exist with a foreign_count of 1.
        assert_eq!(
            conn.query_one::<i64>("SELECT COUNT(*) FROM moz_bookmarks;")
                .unwrap(),
            6
        ); // our 5 roots + new bookmark
        assert_eq!(
            conn.query_one::<i64>(
                "SELECT foreign_count FROM moz_places WHERE url = \"http://example.com/foo\";"
            )
            .unwrap(),
            1
        );
        // visit the bookmark.
        assert!(apply_observation(
            &conn,
            VisitObservation::new(url)
                .with_at(Timestamp::from(727_747_200_001))
                .with_visit_type(VisitType::Link)
        )
        .unwrap()
        .is_some());

        delete_bookmark(&conn, &bookmark_guid).unwrap();
        assert_eq!(
            conn.query_one::<i64>("SELECT COUNT(*) FROM moz_bookmarks;")
                .unwrap(),
            5
        ); // our 5 roots
           // the place should have no foreign references, but still exists.
        assert_eq!(
            conn.query_one::<i64>(
                "SELECT foreign_count FROM moz_places WHERE url = \"http://example.com/foo\";"
            )
            .unwrap(),
            0
        );
        removal_fn(&conn, &place_guid).expect("removal function should work");
        assert_eq!(
            conn.query_one::<i64>("SELECT COUNT(*) FROM moz_places;")
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_one::<i64>("SELECT COUNT(*) FROM moz_origins;")
                .unwrap(),
            0
        );
    }

    // Similar to the above, but if the bookmark has no visits the place/origin should die
    // without requiring history removal
    #[test]
    fn test_visitless_removal_places_and_origins() {
        let conn = new_mem_connection();
        let url = Url::parse("http://example.com/foo").unwrap();
        let bm = InsertableItem::Bookmark {
            b: InsertableBookmark {
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: BookmarkPosition::Append,
                date_added: None,
                last_modified: None,
                guid: None,
                url,
                title: Some("the title".into()),
            },
        };
        assert_eq!(
            conn.query_one::<i64>("SELECT COUNT(*) FROM moz_bookmarks;")
                .unwrap(),
            5
        ); // our 5 roots.
        let bookmark_guid = insert_bookmark(&conn, bm).unwrap();
        // the place should exist with a foreign_count of 1.
        assert_eq!(
            conn.query_one::<i64>("SELECT COUNT(*) FROM moz_bookmarks;")
                .unwrap(),
            6
        ); // our 5 roots + new bookmark
        assert_eq!(
            conn.query_one::<i64>(
                "SELECT foreign_count FROM moz_places WHERE url = \"http://example.com/foo\";"
            )
            .unwrap(),
            1
        );
        // Delete it.
        delete_bookmark(&conn, &bookmark_guid).unwrap();
        assert_eq!(
            conn.query_one::<i64>("SELECT COUNT(*) FROM moz_bookmarks;")
                .unwrap(),
            5
        ); // our 5 roots
           // should be gone from places and origins.
        assert_eq!(
            conn.query_one::<i64>("SELECT COUNT(*) FROM moz_places;")
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_one::<i64>("SELECT COUNT(*) FROM moz_origins;")
                .unwrap(),
            0
        );
    }
}

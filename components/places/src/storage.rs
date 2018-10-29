/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// A "storage" module - this module is intended to be the layer between the
// API and the database.
// This should probably be a sub-directory

use std::{fmt, cmp};
use url::{Url};
use types::{SyncGuid, Timestamp, VisitTransition};
use error::{Result};
use observation::{VisitObservation};
use frecency;

use rusqlite::{Row, Connection};
use rusqlite::{types::{ToSql, FromSql, ToSqlOutput, FromSqlResult, ValueRef}};
use rusqlite::Result as RusqliteResult;

use db::PlacesDb;
use hash;
use sql_support::{self, ConnExt};

// Typesafe way to manage RowIds. Does it make sense? A better way?
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Deserialize, Serialize, Default)]
pub struct RowId(pub i64);

impl From<RowId> for i64 { // XXX - ToSql!
    #[inline]
    fn from(id: RowId) -> Self { id.0 }
}

impl fmt::Display for RowId {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ToSql for RowId {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput> {
        Ok(ToSqlOutput::from(self.0))
    }
}

impl FromSql for RowId {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        value.as_i64().map(|v| RowId(v))
    }
}

#[derive(Debug)]
pub struct PageInfo {
    pub url: Url,
    pub guid: SyncGuid,
    pub row_id: RowId,
    pub title: String,
    pub hidden: bool,
    pub typed: u32,
    pub frecency: i32,
    pub visit_count_local: i32,
    pub visit_count_remote: i32,
    pub last_visit_date_local: Timestamp,
    pub last_visit_date_remote: Timestamp,
}

impl PageInfo {
    pub fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            url: Url::parse(&row.get_checked::<_, Option<String>>("url")?.expect("non null column"))?,
            guid: SyncGuid(row.get_checked::<_, Option<String>>("guid")?.expect("non null column")),
            row_id: row.get_checked("id")?,
            title: row.get_checked::<_, Option<String>>("title")?.unwrap_or_default(),
            hidden: row.get_checked("hidden")?,
            typed: row.get_checked("typed")?,

            frecency:   row.get_checked("frecency")?,
            visit_count_local: row.get_checked("visit_count_local")?,
            visit_count_remote: row.get_checked("visit_count_remote")?,

            last_visit_date_local: row.get_checked::<_, Option<Timestamp>>(
                "last_visit_date_local")?.unwrap_or_default(),
            last_visit_date_remote: row.get_checked::<_, Option<Timestamp>>(
                "last_visit_date_remote")?.unwrap_or_default(),
        })
    }
}

// fetch_page_info gives you one of these.
#[derive(Debug)]
struct FetchedPageInfo {
    page: PageInfo,
    // XXX - not clear what this is used for yet, and whether it should be local, remote or either?
    // The sql below isn't quite sure either :)
    last_visit_id: RowId,
}

impl FetchedPageInfo {
    pub fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            page: PageInfo::from_row(row)?,
            last_visit_id: row.get_checked::<_, Option<RowId>>("last_visit_id")?.expect("No visit id!"),
        })
    }
}

// History::FetchPageInfo
fn fetch_page_info(db: &impl ConnExt, url: &Url) -> Result<Option<FetchedPageInfo>> {
    let sql = "
      SELECT guid, url, id, title, hidden, typed, frecency,
             visit_count_local, visit_count_remote,
             last_visit_date_local, last_visit_date_remote,
      (SELECT id FROM moz_historyvisits
       WHERE place_id = h.id
         AND (visit_date = h.last_visit_date_local OR
              visit_date = h.last_visit_date_remote)) AS last_visit_id
      FROM moz_places h
      WHERE url_hash = hash(:page_url) AND url = :page_url";
    Ok(db.try_query_row(sql, &[(":page_url", &url.clone().into_string())], FetchedPageInfo::from_row, true)?)
}

pub fn apply_observation(db: &mut PlacesDb, visit_ob: VisitObservation) -> Result<()> {
    let tx = db.db.transaction()?;
    apply_observation_direct(tx.conn(), visit_ob)?;
    tx.commit()?;
    Ok(())
}

pub fn apply_observation_direct(db: &Connection, visit_ob: VisitObservation) -> Result<()> {
    let mut page_info = match fetch_page_info(db, &visit_ob.url)? {
        Some(info) => info.page,
        None => new_page_info(db, &visit_ob.url)?,
    };
    let mut updates: Vec<(&str, &str, &ToSql)> = Vec::new();
    if let Some(ref title) = visit_ob.title {
        page_info.title = title.clone();
        updates.push(("title", ":title", &page_info.title));
    }

    let mut update_frecency = false;

    // There's a new visit, so update everything that implies
    if let Some(visit_type) = visit_ob.visit_type {
        // A single non-hidden visit makes the place non-hidden.
        if !visit_ob.get_is_hidden() {
            updates.push(("hidden", ":hidden", &false));
        }
        if visit_type == VisitTransition::Typed {
            page_info.typed += 1;
            updates.push(("typed", ":typed", &page_info.typed));
        }

        let at = visit_ob.at.unwrap_or_else(|| Timestamp::now());
        let is_remote = visit_ob.is_remote.unwrap_or(false);
        add_visit(db, &page_info.row_id, &None, &at, &visit_type, &is_remote)?;
        if is_remote {
            page_info.visit_count_remote += 1;
            updates.push(("visit_count_remote", ":visit_count_remote", &page_info.visit_count_remote));
            page_info.last_visit_date_remote = cmp::max(at, page_info.last_visit_date_remote);
            updates.push(("last_visit_date_remote", ":last_visit_date_remote", &page_info.last_visit_date_remote));
        } else {
            page_info.visit_count_local += 1;
            updates.push(("visit_count_local", ":visit_count_local", &page_info.visit_count_local));
            page_info.last_visit_date_local = cmp::max(at, page_info.last_visit_date_local);
            updates.push(("last_visit_date_local", ":last_visit_date_local", &page_info.last_visit_date_local));
        }
        // a new visit implies new frecency except in error cases.
        if !visit_ob.is_error.unwrap_or(false) {
            update_frecency = true;
        }
    }
    if updates.len() != 0 {
        let mut params: Vec<(&str, &ToSql)> = Vec::with_capacity(updates.len() + 1);
        let mut sets: Vec<String> = Vec::with_capacity(updates.len());
        for (col, name, val) in updates {
            sets.push(format!("{} = {}", col, name));
            params.push((name, val))
        }
        params.push((":row_id", &page_info.row_id.0));
        let sql = format!("UPDATE moz_places
                          SET {}
                          WHERE id == :row_id", sets.join(","));
        db.execute_named_cached(&sql, &params)?;
    }
    // This needs to happen after the other updates.
    if update_frecency {
        page_info.frecency = frecency::calculate_frecency(db,
            &frecency::DEFAULT_FRECENCY_SETTINGS,
            page_info.row_id.0, // TODO: calculate_frecency should take a RowId here.
            Some(visit_ob.get_redirect_frecency_boost()))?;
        let sql = "
            UPDATE moz_places
            SET frecency = :frecency
            WHERE id = :row_id
        ";
        db.execute_named_cached(sql, &[
            (":row_id", &page_info.row_id.0),
            (":frecency", &page_info.frecency),
        ])?;
    }
    Ok(())
}

fn new_page_info(db: &impl ConnExt, url: &Url) -> Result<PageInfo> {
    let guid = super::sync15_adapter::util::random_guid().expect("according to logins-sql, this is fine :)");
    let sql = "INSERT INTO moz_places (guid, url, url_hash)
               VALUES (:guid, :url, hash(:url))";
    db.execute_named_cached(sql, &[
        (":guid", &guid),
        (":url", &url.clone().into_string()),
    ])?;
    Ok(PageInfo {
        url: url.clone(),
        guid: SyncGuid(guid),
        row_id: RowId(db.conn().last_insert_rowid()),
        title: "".into(),
        hidden: true, // will be set to false as soon as a non-hidden visit appears.
        typed: 0,
        frecency: -1,
        visit_count_local: 0,
        visit_count_remote: 0,
        last_visit_date_local: Timestamp(0),
        last_visit_date_remote: Timestamp(0),
    })
}

// Add a single visit - you must know the page rowid. Does not update the
// page info - if you are calling this, you will also need to update the
// parent page with the new visit count, frecency, etc.
fn add_visit(db: &impl ConnExt,
             page_id: &RowId,
             from_visit: &Option<RowId>,
             visit_date: &Timestamp,
             visit_type: &VisitTransition,
             is_local: &bool) -> Result<RowId> {
    let sql =
        "INSERT INTO moz_historyvisits
            (from_visit, place_id, visit_date, visit_type, is_local)
        VALUES (:from_visit, :page_id, :visit_date, :visit_type, :is_local)";
    db.execute_named_cached(sql, &[
        (":from_visit", from_visit),
        (":page_id", page_id),
        (":visit_date", visit_date),
        (":visit_type", visit_type),
        (":is_local", is_local),
    ])?;
    let rid = db.conn().last_insert_rowid();
    Ok(RowId(rid))
}

// Currently not used - we update the frecency as we update the page info.
pub fn update_frecency(db: &mut PlacesDb, id: RowId, redirect: Option<bool>) -> Result<()> {
    let score = frecency::calculate_frecency(db.conn(),
        &frecency::DEFAULT_FRECENCY_SETTINGS,
        id.0, // TODO: calculate_frecency should take a RowId here.
        redirect)?;

    db.execute_named("
        UPDATE moz_places
        SET frecency = :frecency
        WHERE id = :page_id",
        &[(":frecency", &score), (":page_id", &id.0)])?;

    Ok(())
}

pub fn get_visited(db: &PlacesDb, urls: &[Url]) -> Result<Vec<bool>> {
    let mut result = vec![false; urls.len()];
    // Note: this Vec is avoidable in the next rusqlite.
    let url_strs: Vec<&str> = urls.iter().map(|v| v.as_ref()).collect();
    sql_support::each_chunk_mapped(&url_strs, |url| url as &dyn ToSql, |chunk, offset| -> Result<()> {
        let values_with_idx = sql_support::repeat_display(chunk.len(), ",", |i, f|
            write!(f, "({},{},?)", i + offset, hash::hash_url(url_strs[i + offset])));
        let sql = format!("
            WITH to_fetch(fetch_url_index, url_hash, url) AS (VALUES {})
            SELECT fetch_url_index
            FROM moz_places h
            JOIN to_fetch f
            ON h.url_hash = f.url_hash
              AND h.url = f.url
        ", values_with_idx);
        let mut stmt = db.prepare(&sql)?;
        for idx_r in stmt.query_map(chunk, |row| row.get::<_, i64>(0) as usize)? {
            let idx = idx_r?;
            result[idx] = true;
        }
        Ok(())
    })?;
    Ok(result)
}

/// Get the set of urls that were visited between `start` and `end`. Only considers local visits
/// unless you pass in `include_remote`.
pub fn get_visited_urls(db: &PlacesDb, start: Timestamp, end: Timestamp, include_remote: bool) -> Result<Vec<String>> {
    // TODO: if `end` is >= now then we can probably just look at last_visit_date_{local,remote},
    // and avoid touching `moz_historyvisits` at all. That said, this query is taken more or less
    // from what places does so it's probably fine.
    let mut stmt = db.prepare(&format!("
        SELECT h.url
        FROM moz_places h
        WHERE EXISTS (
            SELECT 1 FROM moz_historyvisits v
            WHERE place_id = h.id
                AND visit_date BETWEEN :start AND :end
                {and_is_local}
            LIMIT 1
        )
    ", and_is_local = if include_remote { "" } else { "AND is_local" }))?;

    let iter = stmt.query_map_named(&[
        ("start", &start),
        ("end", &end),
    ], |row| row.get::<_, String>(0))?;

    Ok(iter.collect::<RusqliteResult<Vec<_>>>()?)
}

// Mini experiment with an "Origin" object that knows how to rev_host() itself,
// that I don't want to throw away yet :) I'm really not sure exactly how
// moz_origins fits in TBH :/
#[cfg(test)]
mod tests {
    use super::*;

    struct Origin {
        prefix: String,
        host: String,
        frecency: i64,
    }
    impl Origin {
        pub fn rev_host(&self) -> String {
            // Note: this is consistent with how places handles hosts, and our `reverse_host`
            // function. We explictly don't want to use unicode_segmentation because it's not
            // stable across unicode versions, and valid hosts are expected to be strings.
            // (The `url` crate will punycode them for us).
            String::from_utf8(self.host.bytes().rev().map(|b|
                b.to_ascii_lowercase()).collect::<Vec<_>>())
                .unwrap() // TODO: We should return a Result, or punycode on construction if needed.
        }
    }

    #[test]
    fn test_reverse() {
        let o = Origin {prefix: "http".to_string(),
                        host: "foo.com".to_string(),
                        frecency: 0 };
        assert_eq!(o.prefix, "http");
        assert_eq!(o.frecency, 0);
        assert_eq!(o.rev_host(), "moc.oof");
    }

    #[test]
    fn test_get_visited() {
        let mut conn = PlacesDb::open_in_memory(None).expect("no memory db");

        let to_add = [
            "https://www.example.com/1",
            "https://www.example.com/12",
            "https://www.example.com/123",
            "https://www.example.com/1234",
            "https://www.mozilla.com",
            "https://www.firefox.com",
        ];

        for item in &to_add {
            apply_observation(&mut conn, VisitObservation::new(Url::parse(item).unwrap()))
                .expect("Should apply visit");
        }

        let to_search = [
            ("https://www.example.com", false),
            ("https://www.example.com/1", true),
            ("https://www.example.com/12", true),
            ("https://www.example.com/123", true),
            ("https://www.example.com/1234", true),
            ("https://www.example.com/12345", false),
            ("https://www.mozilla.com", true),
            ("https://www.firefox.com", true),
            ("https://www.mozilla.org", false),
            // dupes should still work!
            ("https://www.example.com/1234", true),
            ("https://www.example.com/12345", false),
        ];

        let urls = to_search.iter()
            .map(|(url, _expect)| Url::parse(url).unwrap())
            .collect::<Vec<_>>();

        let visited = get_visited(&conn, &urls).unwrap();

        assert_eq!(visited.len(), to_search.len());

        for (i, &did_see) in visited.iter().enumerate() {
            assert_eq!(did_see, to_search[i].1,
                "Wrong value in get_visited for '{}' (idx {}), want {}, have {}",
                to_search[i].0, i, // idx is logged because some things are repeated
                to_search[i].1, did_see);
        }
    }
}

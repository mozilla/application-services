/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// A "storage" module - this module is intended to be the layer between the
// API and the database.
// This should probably be a sub-directory

use std::{fmt};
use url::{Url};
use types::{SyncGuid, Timestamp, VisitTransition};
use error::{Result};

use rusqlite::{Row};
use rusqlite::{types::{ToSql, FromSql, ToSqlOutput, FromSqlResult, ValueRef}};
use rusqlite::Result as RusqliteResult;

use ::db::PlacesDb;

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

// A PageId is an enum, either a Guid or a Url, to identify a unique key for a
// page as specifying both doesn't make sense (and specifying the guid is
// slighly faster, plus sync might *only* have the guid?)
#[derive(Debug)]
pub enum PageId {
    Guid(SyncGuid),
    Url(Url),
}

// fetch_page_info gives you one of these.
#[derive(Debug)]
pub struct FetchedPageInfo {
    pub page_id: PageId,
    pub row_id: RowId,
    pub url: Url,
    pub title: String,
    pub hidden: bool,
    pub typed: u32,
    pub frecency: i32,
    pub visit_count_local: i32,
    pub visit_count_remote: i32,
    pub last_visit_date_local: Timestamp,
    pub last_visit_date_remote: Timestamp,
    // XXX - not clear what this is used for yet, and whether it should be local, remote or either?
    // The sql below isn't quite sure either :)
    pub last_visit_id: RowId,
}

impl FetchedPageInfo {
    pub fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            page_id: PageId::Guid(row.get_checked("guid")?),
            url: Url::parse(&row.get_checked::<_, Option<String>>("url")?.expect("non null column"))?,
            row_id: row.get_checked("id")?,
            title: row.get_checked("title")?,
            hidden: row.get_checked("hidden")?,
            typed: row.get_checked("typed")?,

            frecency:   row.get_checked("frecency")?,
            visit_count_local: row.get_checked("visit_count_local")?,
            visit_count_remote: row.get_checked("visit_count_remote")?,

            last_visit_date_local: row.get_checked("last_visit_date_local")?,
            last_visit_date_remote: row.get_checked("last_visit_date_remote")?,
            last_visit_id: row.get_checked("last_visit_id")?,
        })
    }
}

// History::FetchPageInfo
pub fn fetch_page_info(db: &PlacesDb, page_id: &PageId) -> Result<Option<FetchedPageInfo>> {
    Ok(match page_id {
        // XXX - there's way too much sql and db.query duplicated here!?
        PageId::Guid(ref guid) => {
            let sql = "
              SELECT guid, url, id, title, hidden, typed, frecency,
                     visit_count_local, visit_count_remote,
                     last_visit_date_local, last_visit_date_remote,
              (SELECT id FROM moz_historyvisits
               WHERE place_id = h.id AND visit_date = h.last_visit_date_local) AS last_visit_id
              FROM moz_places h
              WHERE guid = :guid";
            db.query_row_named(sql, &[(":guid", guid)], FetchedPageInfo::from_row)?
        },
        PageId::Url(url) => {
            let sql = "
              SELECT guid, url, id, title, hidden, typed, frecency,
                     visit_count_local, visit_count_remote,
                     last_visit_date_local, last_visit_date_remote,
              (SELECT id FROM moz_historyvisits
               WHERE place_id = h.id AND visit_date = h.last_visit_date_local) AS last_visit_id
              FROM moz_places h
              WHERE url_hash = hash(:page_url) AND url = :page_url";
            db.query_row_named(sql, &[(":page_url", &url.clone().into_string())], FetchedPageInfo::from_row)?
        }
    })
}

// What you need to supply when calling new_page_info()
#[derive(Debug)]
pub struct NewPageInfo {
    pub url: Url,
    pub title: Option<String>,
    pub hidden: bool,
    pub typed: u32,
}

pub fn new_page_info(db: &PlacesDb, pi: &NewPageInfo) -> Result<(PageId, RowId)> {
    let sql = "
        INSERT INTO moz_places
        (url, url_hash, title, hidden, typed, frecency, guid)
        VALUES (:url, hash(:url), :title, :hidden, :typed, :frecency, :guid)";

    let guid = super::sync::util::random_guid().expect("according to logins-sql, this is fine :)");
    db.execute_named_cached(sql, &[
        (":url", &pi.url.clone().into_string()),
        (":title", &pi.title),
        (":hidden", &pi.hidden),
        (":typed", &pi.typed),
        (":frecency", &-1),
        (":guid", &guid),
    ])?;
    Ok((PageId::Guid(SyncGuid(guid)), RowId(db.db.last_insert_rowid())))
}

// Add a single visit - you must know the page rowid.
// (Why so many params here? A struct?)
pub fn add_visit(db: &PlacesDb,
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
    let rid = db.db.last_insert_rowid();
    Ok(RowId(rid))
}

// Mini experiment with an "Origin" object that knows how to rev_host() itself,
// that I don't want to throw away yet :) I'm really not sure exactly how
// moz_origins fits in TBH :/
#[cfg(test)]
mod tests {
    use unicode_segmentation::UnicodeSegmentation;

    struct Origin {
        prefix: String,
        host: String,
        frecency: i64,
    }
    impl Origin {
        pub fn rev_host(&self) -> String {
            self.host.graphemes(true).rev().flat_map(|g| g.chars()).collect()
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

}

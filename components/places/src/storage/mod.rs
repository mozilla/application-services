/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// A "storage" module - this module is intended to be the layer between the
// API and the database.

pub mod bookmarks;
pub mod history;

use crate::error::Result;
use crate::types::{SyncGuid, SyncStatus, Timestamp};
use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::Result as RusqliteResult;
use rusqlite::Row;
use serde_derive::*;
use sql_support::{self, ConnExt};
use std::fmt;
use url::Url;

// Typesafe way to manage RowIds. Does it make sense? A better way?
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Deserialize, Serialize, Default)]
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
    pub sync_status: SyncStatus,
    pub sync_change_counter: u32,
}

impl PageInfo {
    pub fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            url: Url::parse(&row.get_checked::<_, String>("url")?)?,
            guid: SyncGuid(row.get_checked::<_, String>("guid")?),
            row_id: row.get_checked("id")?,
            title: row
                .get_checked::<_, Option<String>>("title")?
                .unwrap_or_default(),
            hidden: row.get_checked("hidden")?,
            typed: row.get_checked("typed")?,

            frecency: row.get_checked("frecency")?,
            visit_count_local: row.get_checked("visit_count_local")?,
            visit_count_remote: row.get_checked("visit_count_remote")?,

            last_visit_date_local: row
                .get_checked::<_, Option<Timestamp>>("last_visit_date_local")?
                .unwrap_or_default(),
            last_visit_date_remote: row
                .get_checked::<_, Option<Timestamp>>("last_visit_date_remote")?
                .unwrap_or_default(),

            sync_status: SyncStatus::from_u8(row.get_checked::<_, u8>("sync_status")?),
            sync_change_counter: row
                .get_checked::<_, Option<u32>>("sync_change_counter")?
                .unwrap_or_default(),
        })
    }
}

// fetch_page_info gives you one of these.
#[derive(Debug)]
struct FetchedPageInfo {
    page: PageInfo,
    // XXX - not clear what this is used for yet, and whether it should be local, remote or either?
    // The sql below isn't quite sure either :)
    last_visit_id: Option<RowId>,
}

impl FetchedPageInfo {
    pub fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            page: PageInfo::from_row(row)?,
            last_visit_id: row.get_checked::<_, Option<RowId>>("last_visit_id")?,
        })
    }
}

// History::FetchPageInfo
fn fetch_page_info(db: &impl ConnExt, url: &Url) -> Result<Option<FetchedPageInfo>> {
    let sql = "
      SELECT guid, url, id, title, hidden, typed, frecency,
             visit_count_local, visit_count_remote,
             last_visit_date_local, last_visit_date_remote,
             sync_status, sync_change_counter,
             (SELECT id FROM moz_historyvisits
              WHERE place_id = h.id
                AND (visit_date = h.last_visit_date_local OR
                     visit_date = h.last_visit_date_remote)) AS last_visit_id
      FROM moz_places h
      WHERE url_hash = hash(:page_url) AND url = :page_url";
    Ok(db.try_query_row(
        sql,
        &[(":page_url", &url.clone().into_string())],
        FetchedPageInfo::from_row,
        true,
    )?)
}

fn new_page_info(db: &impl ConnExt, url: &Url, new_guid: Option<SyncGuid>) -> Result<PageInfo> {
    let guid = match new_guid {
        Some(guid) => guid,
        None => SyncGuid::new(),
    };
    let sql = "INSERT INTO moz_places (guid, url, url_hash)
               VALUES (:guid, :url, hash(:url))";
    db.execute_named_cached(
        sql,
        &[(":guid", &guid), (":url", &url.clone().into_string())],
    )?;
    Ok(PageInfo {
        url: url.clone(),
        guid,
        row_id: RowId(db.conn().last_insert_rowid()),
        title: "".into(),
        hidden: true, // will be set to false as soon as a non-hidden visit appears.
        typed: 0,
        frecency: -1,
        visit_count_local: 0,
        visit_count_remote: 0,
        last_visit_date_local: Timestamp(0),
        last_visit_date_remote: Timestamp(0),
        sync_status: SyncStatus::New,
        sync_change_counter: 0,
    })
}

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use interrupt_support::SqlInterruptHandle;
use rusqlite::{
    named_params,
    types::{FromSql, ToSql},
    Connection, OpenFlags,
};
use sql_support::{open_database::open_database_with_flags, ConnExt};

use crate::{
    keyword::full_keyword, schema::SuggestConnectionInitializer, RemoteRecordId, RemoteSuggestion,
    Result, Suggestion,
};

pub const LAST_FETCH_META_KEY: &str = "last_fetch";
pub const NONSPONSORED_IAB_CATEGORIES: &[&str] = &["5 - Education"];

#[derive(Clone, Copy)]
pub enum ConnectionType {
    ReadOnly,
    ReadWrite,
}

impl From<ConnectionType> for OpenFlags {
    fn from(type_: ConnectionType) -> Self {
        match type_ {
            ConnectionType::ReadOnly => {
                OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_READ_ONLY
            }
            ConnectionType::ReadWrite => {
                OpenFlags::SQLITE_OPEN_URI
                    | OpenFlags::SQLITE_OPEN_CREATE
                    | OpenFlags::SQLITE_OPEN_READ_WRITE
            }
        }
    }
}

pub struct SuggestDb {
    conn: Mutex<Connection>,
    pub type_: ConnectionType,
    pub interrupt_handle: Arc<SqlInterruptHandle>,
}

impl SuggestDb {
    pub fn open(path: impl AsRef<Path>, type_: ConnectionType) -> Result<Self> {
        let conn = open_database_with_flags(path, type_.into(), &SuggestConnectionInitializer)?;
        Ok(Self::with_connection(conn, type_))
    }

    fn with_connection(conn: Connection, type_: ConnectionType) -> Self {
        let interrupt_handle = Arc::new(SqlInterruptHandle::new(&conn));
        Self {
            conn: Mutex::new(conn),
            type_,
            interrupt_handle,
        }
    }

    pub fn fetch_by_keyword(&self, keyword: &str) -> Result<Vec<Suggestion>> {
        let conn = self.conn.lock().unwrap();
        conn.query_rows_and_then_cached(
            "SELECT s.id, k.rank, s.block_id, s.advertiser, s.iab_category,
                    s.title, s.url, s.impression_url, s.click_url,
                    (SELECT i.data FROM icons i WHERE i.id = s.icon_id) AS icon
             FROM suggestions s
             JOIN keywords k ON k.suggestion_id = s.id
             WHERE k.keyword = :keyword
             LIMIT 1",
            named_params! {
                ":keyword": keyword,
            },
            |row| -> Result<Suggestion> {
                let keywords: Vec<String> = conn.query_rows_and_then(
                    "SELECT keyword FROM keywords
                     WHERE suggestion_id = :suggestion_id AND rank >= :rank
                     ORDER BY rank ASC",
                    named_params! {
                        ":suggestion_id": row.get::<_, i64>("id")?,
                        ":rank": row.get::<_, i64>("rank")?,
                    },
                    |row| row.get(0),
                )?;
                let iab_category = row.get::<_, String>("iab_category")?;
                let is_sponsored = !NONSPONSORED_IAB_CATEGORIES.contains(&iab_category.as_str());
                Ok(Suggestion {
                    block_id: row.get("block_id")?,
                    advertiser: row.get("advertiser")?,
                    iab_category,
                    is_sponsored,
                    title: row.get("title")?,
                    url: row.get("url")?,
                    full_keyword: full_keyword(keyword, &keywords),
                    icon: row.get("icon")?,
                    impression_url: row.get("impression_url")?,
                    click_url: row.get("click_url")?,
                })
            },
        )
    }

    pub fn ingest(
        &self,
        record_id: &RemoteRecordId,
        suggestions: &[RemoteSuggestion],
    ) -> Result<()> {
        let scope = self.interrupt_handle.begin_interrupt_scope()?;
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for suggestion in suggestions {
            scope.err_if_interrupted()?;
            let suggestion_id: i64 = tx.query_row_and_then_cachable(
                "INSERT INTO suggestions(
                     record_id,
                     block_id,
                     advertiser,
                     iab_category,
                     title,
                     url,
                     icon_id,
                     impression_url,
                     click_url
                 )
                 VALUES(
                     :record_id,
                     :block_id,
                     :advertiser,
                     :iab_category,
                     :title,
                     :url,
                     :icon_id,
                     :impression_url,
                     :click_url
                 )
                 RETURNING id",
                named_params! {
                    ":record_id": record_id.as_str(),
                    ":block_id": suggestion.block_id,
                    ":advertiser": suggestion.advertiser,
                    ":iab_category": suggestion.iab_category,
                    ":title": suggestion.title,
                    ":url": suggestion.url,
                    ":icon_id": suggestion.icon_id,
                    ":impression_url": suggestion.impression_url,
                    ":click_url": suggestion.click_url,
                },
                |row| row.get(0),
                true,
            )?;
            for (index, keyword) in suggestion.keywords.iter().enumerate() {
                tx.execute(
                    "INSERT INTO keywords(
                         keyword,
                         suggestion_id,
                         rank
                     )
                     VALUES(
                         :keyword,
                         :suggestion_id,
                         :rank
                     )",
                    named_params! {
                        ":keyword": keyword,
                        ":rank": index,
                        ":suggestion_id": suggestion_id,
                    },
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // ...
    pub fn put_icon(&self, icon_id: &str, data: &[u8]) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO icons(
                 id,
                 data
             )
             VALUES(
                 :id,
                 :data
             )",
            named_params! {
                ":id": icon_id,
                ":data": data,
            },
        )?;
        Ok(())
    }

    // ...
    pub fn drop(&self, record_id: &RemoteRecordId) -> Result<()> {
        self.conn.lock().unwrap().execute_cached(
            "DELETE FROM suggestions WHERE record_id = :record_id",
            named_params! { ":record_id": record_id.as_str() },
        )?;
        Ok(())
    }

    // ...
    pub fn drop_icon(&self, icon_id: &str) -> Result<()> {
        self.conn.lock().unwrap().execute_cached(
            "DELETE FROM icons WHERE id = :id",
            named_params! { ":id": icon_id },
        )?;
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        self.conn.lock().unwrap().execute_batch(
            "DELETE FROM suggestions;
             DELETE FROM meta;",
        )?;
        Ok(())
    }

    pub fn get_meta<T: FromSql>(&self, key: &str) -> Result<Option<T>> {
        Ok(self.conn.lock().unwrap().try_query_one(
            "SELECT value FROM meta WHERE key = :key",
            named_params! { ":key": key },
            true,
        )?)
    }

    pub fn put_meta(&self, key: &str, value: impl ToSql) -> Result<()> {
        self.conn.lock().unwrap().execute_cached(
            "REPLACE INTO meta(key, value) VALUES(:key, :value)",
            named_params! { ":key": key, ":value": value },
        )?;
        Ok(())
    }
}

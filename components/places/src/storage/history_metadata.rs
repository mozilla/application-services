/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::{PlacesDb, PlacesTransaction};
use crate::error::Result;
use crate::msg_types::{HistoryMetadata, HistoryMetadataList};
use sql_support::{self, ConnExt};
use sync_guid::Guid as SyncGuid;
use types::Timestamp;
use url::Url;

use lazy_static::lazy_static;

enum SearchQueryEntry {
    None,
    Existing(i64),
    CreateFor(String),
}

enum ParentDomainEntry {
    None,
    Existing(i64),
    CreateFor(Url),
}

const MAX_QUERY_RESULTS: i32 = 1000;

const COMMON_METADATA_SELECT: &str = "
SELECT
    m.guid as guid, p.url as url, p.title as title, m.created_at as created_at,
    m.updated_at as updated_at, m.total_view_time as total_view_time,
    m.is_media as is_media, o.host as parent_domain, s.term as search_term
FROM moz_places_metadata m
LEFT JOIN moz_places p ON m.place_id = p.id
LEFT JOIN moz_places_metadata_search_queries s ON m.search_query_id = s.id
LEFT JOIN moz_origins o ON o.id = m.parent_domain_id";

lazy_static! {
    static ref GET_LATEST_SQL: String = format!(
        "{common_select_sql}
        WHERE p.url_hash = hash(:url) AND url = :url
        ORDER BY updated_at DESC
        LIMIT 1",
        common_select_sql = COMMON_METADATA_SELECT
    );
    static ref GET_BETWEEN_SQL: String = format!(
        "{common_select_sql}
        WHERE updated_at BETWEEN :start AND :end
        ORDER BY updated_at DESC
        LIMIT {max_limit}",
        common_select_sql = COMMON_METADATA_SELECT,
        max_limit = MAX_QUERY_RESULTS
    );
    static ref GET_SINCE_SQL: String = format!(
        "{common_select_sql}
        WHERE updated_at >= :start
        ORDER BY updated_at DESC
        LIMIT {max_limit}",
        common_select_sql = COMMON_METADATA_SELECT,
        max_limit = MAX_QUERY_RESULTS
    );
    static ref QUERY_SQL: String = format!(
        "{common_select_sql}
        WHERE
            url LIKE :query OR
            title LIKE :query OR
            search_term LIKE :query
        ORDER BY total_view_time DESC
        LIMIT :limit",
        common_select_sql = COMMON_METADATA_SELECT
    );
}

pub fn get_latest_for_url(db: &PlacesDb, url: &Url) -> Result<Option<HistoryMetadata>> {
    let metadata = db.try_query_row(
        GET_LATEST_SQL.as_str(),
        &[(":url", &url.as_str())],
        HistoryMetadata::from_row,
        true,
    )?;
    Ok(metadata)
}

pub fn get_between(db: &PlacesDb, start: i64, end: i64) -> Result<HistoryMetadataList> {
    let metadata = db.query_rows_and_then_named_cached(
        GET_BETWEEN_SQL.as_str(),
        rusqlite::named_params! {
            ":start": start,
            ":end": end,
        },
        HistoryMetadata::from_row,
    )?;
    Ok(HistoryMetadataList { metadata })
}

pub fn get_since(db: &PlacesDb, start: i64) -> Result<HistoryMetadataList> {
    let metadata = db.query_rows_and_then_named_cached(
        GET_SINCE_SQL.as_str(),
        rusqlite::named_params! {
            ":start": start
        },
        HistoryMetadata::from_row,
    )?;
    Ok(HistoryMetadataList { metadata })
}

pub fn query(db: &PlacesDb, query: &str, limit: i64) -> Result<HistoryMetadataList> {
    let metadata = db.query_rows_and_then_named_cached(
        QUERY_SQL.as_str(),
        rusqlite::named_params! {
            ":query": format!("%{}%", query),
            ":limit": limit
        },
        HistoryMetadata::from_row,
    )?;
    Ok(HistoryMetadataList { metadata })
}

fn insert_metadata_in_tx(
    tx: &PlacesTransaction<'_>,
    place_id: Option<i64>,
    search_query_entry: SearchQueryEntry,
    parent_domain_entry: ParentDomainEntry,
    metadata: HistoryMetadata,
) -> Result<SyncGuid> {
    // If HistoryMetadata already has a guid, that's an error.
    assert!(metadata.guid.is_none());

    // Heavy lifting around moz_places inserting (e.g. updating moz_origins, frecency, etc) is performed via triggers.
    let places_id = match place_id {
        Some(id) => id,
        None => {
            let sql = "INSERT INTO moz_places (guid, url, title, url_hash)
               VALUES (:guid, :url, :title, hash(:url))";

            let guid = SyncGuid::random();

            // This normalizes urls, e.g. adding trailing slashes when they're missing.
            // We do the same when querying, to make sure we have a consistent representation
            // of urls as they're flowing around.
            let url = Url::parse(&metadata.url)?;

            tx.execute_named_cached(
                sql,
                &[
                    (":guid", &guid),
                    (":url", &url.as_str()),
                    (":title", &metadata.title),
                ],
            )?;
            tx.conn().last_insert_rowid()
        }
    };

    let search_query_id = match search_query_entry {
        SearchQueryEntry::None => None,
        SearchQueryEntry::Existing(id) => Some(id),
        SearchQueryEntry::CreateFor(term) => {
            tx.execute_named_cached(
                "INSERT INTO moz_places_metadata_search_queries(term) VALUES (:term)",
                &[(":term", &term)],
            )?;
            Some(tx.conn().last_insert_rowid())
        }
    };

    let parent_domain_id = match parent_domain_entry {
        ParentDomainEntry::None => None,
        ParentDomainEntry::Existing(id) => Some(id),
        ParentDomainEntry::CreateFor(url) => {
            tx.execute_named_cached(
                "INSERT INTO moz_origins (prefix, host, rev_host, frecency)
            VALUES (
                get_prefix(:url),
                get_host_and_port(:url),
                reverse_host(get_host_and_port(:url)),
                -1
            )",
                &[(":url", &url.as_str())],
            )?;
            Some(tx.conn().last_insert_rowid())
        }
    };

    let sql = "INSERT INTO moz_places_metadata
        (guid, place_id, created_at, updated_at, total_view_time, search_query_id, is_media, parent_domain_id)
    VALUES
        (:guid, :place_id, :created_at, :updated_at, :total_view_time, :search_query_id, :is_media, :parent_domain_id)";

    let guid = SyncGuid::random();
    tx.execute_named_cached(
        sql,
        &[
            (":guid", &guid),
            (":place_id", &places_id),
            (":created_at", &metadata.created_at), // we probably just want to auto-generate these.
            (":updated_at", &metadata.updated_at),
            (":total_view_time", &metadata.total_view_time),
            (":search_query_id", &search_query_id),
            (":is_media", &metadata.is_media),
            (":parent_domain_id", &parent_domain_id),
        ],
    )?;
    Ok(guid)
}

pub fn add_metadata(db: &PlacesDb, metadata: HistoryMetadata) -> Result<SyncGuid> {
    let places_id = db.try_query_one(
        "SELECT id FROM moz_places WHERE url_hash = hash(:url) AND url = :url",
        &[(":url", &metadata.url)],
        true,
    )?;

    // Look up the search query first, maybe it's already stored in the db.
    // Do this before starting a transaction, since it's a SELECT and doesn't need to be part of a tx.
    // We depend on having a single write connection, so we know a search term won't be inserted by another caller.
    // NB: there is also a sync writer, but we don't currently sync any of this data.
    let search_query = match &metadata.search_term {
        None => SearchQueryEntry::None,
        Some(term) => {
            let lowercase_term = term.to_lowercase();
            match db.try_query_one(
                "SELECT id FROM moz_places_metadata_search_queries WHERE term = :term",
                &[(":term", &lowercase_term)],
                true,
            )? {
                Some(id) => SearchQueryEntry::Existing(id),
                None => SearchQueryEntry::CreateFor(lowercase_term),
            }
        }
    };

    let parent_domain = match &metadata.parent_url {
        None => ParentDomainEntry::None,
        Some(parent_url) => {
            let parent_url = Url::parse(parent_url)?;
            match db.try_query_one(
                "SELECT id FROM moz_origins WHERE prefix = get_prefix(:url) AND host = get_host_and_port(:url)",
                &[(":url", &parent_url.as_str())],
                true
            )? {
                Some(id) => ParentDomainEntry::Existing(id),
                None => ParentDomainEntry::CreateFor(parent_url)
            }
        }
    };

    let tx = db.begin_transaction()?;
    let result = insert_metadata_in_tx(&tx, places_id, search_query, parent_domain, metadata);
    // Inserting into moz_places has side-effects (temp tables are populated via triggers and need to be flushed).
    // This call "finalizes" these side-effects.
    super::delete_pending_temp_tables(db)?;
    match result {
        Ok(_) => tx.commit()?,
        Err(_) => tx.rollback()?,
    };
    result
}

pub fn update_metadata(db: &PlacesDb, guid: &SyncGuid, total_view_time: i32) -> Result<()> {
    let now = Timestamp::now();
    db.execute_named_cached(
        "UPDATE moz_places_metadata
         SET total_view_time = :total_view_time, updated_at = :updated_at
         WHERE guid = :guid",
        &[
            (":guid", guid),
            (":total_view_time", &total_view_time),
            (":updated_at", &now),
        ],
    )?;
    Ok(())
}

pub fn delete_older_than(db: &PlacesDb, older_than: i64) -> Result<()> {
    db.execute_named_cached(
        "DELETE FROM moz_places_metadata
         WHERE updated_at < :older_than",
        &[(":older_than", &older_than)],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::ConnectionType;
    use pretty_assertions::assert_eq;
    use types::Timestamp;

    #[macro_use]
    macro_rules! add_and_assert_history_metadata {
        ($conn:expr, url $url:expr, title $title:expr, created_at $created_at:expr, updated_at $updated_at:expr, total_time $tvt:expr, search_term $search_term:expr, is_media $is_media:expr, parent_url $parent_url:expr, parent_domain $parent_domain:expr) => {
            // Create an object to add.
            let metadata = HistoryMetadata {
                guid: None,
                url: String::from($url),
                title: match $title as Option<&str> {
                    Some(t) => Some(String::from(t)),
                    None => None
                },
                created_at: $created_at,
                updated_at: $updated_at,
                total_view_time: $tvt,
                search_term: match $search_term as Option<&str> {
                    Some(t) => Some(String::from(t)),
                    None => None
                },
                is_media: $is_media,
                parent_url: match $parent_url as Option<&str> {
                    Some(t) => Some(String::from(t)),
                    None => None
                }
            };
            // Add it.
            let db_guid = add_metadata($conn, metadata).expect("should add metadata");

            // Fetch it back and compare results.
            let m = get_latest_for_url($conn, &Url::parse($url).expect("url parse")).expect("get by url should work");
            let meta = m.expect("metadata record must be present");

            assert_eq!(db_guid, meta.guid.clone().expect("must have a guid"));
            assert_history_metadata_record!(meta, url $url, title $title, created_at $created_at, updated_at $updated_at, total_time $tvt, search_term $search_term, is_media $is_media, parent_url $parent_url, parent_domain $parent_domain);
        };
    }

    #[macro_use]
    macro_rules! assert_history_metadata_record {
        ($record:expr, url $url:expr, title $title:expr, created_at $created_at:expr, updated_at $updated_at:expr, total_time $tvt:expr, search_term $search_term:expr, is_media $is_media:expr, parent_url $parent_url:expr, parent_domain $parent_domain:expr) => {
            assert_eq!(String::from($url), $record.url, "url must match");
            assert_eq!($created_at, $record.created_at, "created_at must match");
            assert_eq!($updated_at, $record.updated_at, "updated_at must match");
            assert_eq!($tvt, $record.total_view_time, "total_view_time must match");
            assert_eq!($is_media, $record.is_media, "is_media must match");

            let meta = $record.clone(); // ugh... not sure why this `clone` is necessary.

            match $title as Option<&str> {
                Some(t) => assert_eq!(
                    String::from(t),
                    meta.title.expect("title must be Some"),
                    "title must match"
                ),
                None => assert_eq!(true, meta.title.is_none(), "title expected to be None"),
            };
            match $search_term as Option<&str> {
                Some(t) => assert_eq!(
                    String::from(t),
                    meta.search_term.expect("search_term must be Some"),
                    "search_term must match"
                ),
                None => assert_eq!(
                    true,
                    meta.search_term.is_none(),
                    "search_term expected to be None"
                ),
            };
            match $parent_domain as Option<&str> {
                Some(t) => assert_eq!(
                    String::from(t),
                    meta.parent_url.expect("parent_url must be Some"),
                    "parent_url must match"
                ),
                None => assert_eq!(
                    true,
                    meta.parent_url.is_none(),
                    "parent_url expected to be None"
                ),
            };
        };
    }

    #[test]
    fn test_get_latest_for_url() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        let now: Timestamp = std::time::SystemTime::now().into();
        let now_i64 = now.0 as i64;

        add_and_assert_history_metadata!(
            &conn,
            url "http://mozilla.com/",
            title Some("Sample test page"),
            created_at now_i64 + 10000,
            updated_at now_i64 + 10000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url None,
            parent_domain None
        );

        add_and_assert_history_metadata!(
            &conn,
            url "http://www.mozilla.org/test",
            title Some("Another page title"),
            created_at now_i64,
            updated_at now_i64,
            total_time 10000,
            search_term Some("test search"),
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=some+url+ooook"),
            parent_domain Some("www.google.com")
        );

        add_and_assert_history_metadata!(
            &conn,
            url "http://www.mozilla.org/test2",
            title Some("Some page title"),
            created_at now_i64 + 500,
            updated_at now_i64 + 500,
            total_time 15000,
            search_term Some("another search"),
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=another+url+ooook"),
            parent_domain Some("www.google.com")
        );

        add_and_assert_history_metadata!(
            &conn,
            url "http://www.example.com/video",
            title Some("Sample test page"),
            created_at now_i64 + 15000,
            updated_at now_i64 + 25000,
            total_time 200000,
            search_term None,
            is_media true,
            parent_url Some("https://www.reuters.com/"),
            parent_domain Some("www.reuters.com")
        );
    }

    #[test]
    fn test_get_between() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        let now: Timestamp = std::time::SystemTime::now().into();
        let now_i64 = now.0 as i64;

        add_and_assert_history_metadata!(
            &conn,
            url "http://mozilla.com/1",
            title Some("Test page 1"),
            created_at now_i64,
            updated_at now_i64 + 10000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url None,
            parent_domain None
        );

        add_and_assert_history_metadata!(
            &conn,
            url "http://mozilla.com/2",
            title Some("Test page 2"),
            created_at now_i64,
            updated_at now_i64 + 20000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url None,
            parent_domain None
        );

        add_and_assert_history_metadata!(
            &conn,
            url "http://mozilla.com/3",
            title Some("Test page 3"),
            created_at now_i64,
            updated_at now_i64 + 30000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url None,
            parent_domain None
        );

        let start = now_i64 + 10001;
        let end = now_i64 + 29999;
        let meta = get_between(&conn, start, end).expect("expected results");

        assert_eq!(1, meta.metadata.len(), "expected exactly one result");

        let result = meta.metadata.first().expect("expected one result");

        assert_eq!("http://mozilla.com/2", result.url, "url must match");
    }

    #[test]
    fn test_get_since() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        let now: Timestamp = std::time::SystemTime::now().into();
        let now_i64 = now.0 as i64;

        add_and_assert_history_metadata!(
            &conn,
            url "http://mozilla.com/1",
            title Some("Test page 1"),
            created_at now_i64,
            updated_at now_i64 + 10000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url None,
            parent_domain None
        );

        add_and_assert_history_metadata!(
            &conn,
            url "http://mozilla.com/2",
            title Some("Test page 2"),
            created_at now_i64,
            updated_at now_i64 + 20000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url None,
            parent_domain None
        );

        add_and_assert_history_metadata!(
            &conn,
            url "http://mozilla.com/3",
            title Some("Test page 3"),
            created_at now_i64,
            updated_at now_i64 + 30000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url None,
            parent_domain None
        );

        let start = now_i64 + 10001;
        let meta = get_since(&conn, start).expect("expected results");

        assert_eq!(2, meta.metadata.len(), "expected exactly one result");

        let first_result = &meta.metadata[0];
        assert_eq!("http://mozilla.com/3", first_result.url, "url must match");

        let second_result = &meta.metadata[1];
        assert_eq!("http://mozilla.com/2", second_result.url, "url must match");
    }

    #[test]
    fn test_query() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        let now: Timestamp = std::time::SystemTime::now().into();
        let now_i64 = now.0 as i64;

        add_and_assert_history_metadata!(
            &conn,
            url "https://www.cbc.ca/news/politics/federal-budget-2021-freeland-zimonjic-1.5991021",
            title Some("Budget vows to build &#x27;for the long term&#x27; as it promises child care cash, projects massive deficits | CBC News"),
            created_at now_i64,
            updated_at now_i64 + 10000,
            total_time 20000,
            search_term Some("cbc federal budget 2021"),
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            parent_domain Some("www.google.com")
        );

        add_and_assert_history_metadata!(
            &conn,
            url "https://stackoverflow.com/questions/37777675/how-to-create-a-formatted-string-out-of-a-literal-in-rust",
            title Some("formatting - How to create a formatted String out of a literal in Rust? - Stack Overflow"),
            created_at now_i64,
            updated_at now_i64 + 10000,
            total_time 20000,
            search_term Some("rust string format"),
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            parent_domain Some("www.google.com")
        );

        add_and_assert_history_metadata!(
            &conn,
            url "https://www.sqlite.org/lang_corefunc.html#instr",
            title Some("Built-In Scalar SQL Functions"),
            created_at now_i64,
            updated_at now_i64 + 10000,
            total_time 20000,
            search_term Some("sqlite like"),
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            parent_domain Some("www.google.com")
        );

        // query by title
        let meta = query(&conn, "child care", 10).expect("query should work");
        assert_eq!(1, meta.metadata.len(), "expected exactly one result");
        assert_history_metadata_record!(&meta.metadata[0],
            url "https://www.cbc.ca/news/politics/federal-budget-2021-freeland-zimonjic-1.5991021",
            title Some("Budget vows to build &#x27;for the long term&#x27; as it promises child care cash, projects massive deficits | CBC News"),
            created_at now_i64,
            updated_at now_i64 + 10000,
            total_time 20000,
            search_term Some("cbc federal budget 2021"),
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            parent_domain Some("www.google.com")
        );

        // query by search term
        let meta = query(&conn, "string format", 10).expect("query should work");
        assert_eq!(1, meta.metadata.len(), "expected exactly one result");
        assert_history_metadata_record!(&meta.metadata[0],
            url "https://stackoverflow.com/questions/37777675/how-to-create-a-formatted-string-out-of-a-literal-in-rust",
            title Some("formatting - How to create a formatted String out of a literal in Rust? - Stack Overflow"),
            created_at now_i64,
            updated_at now_i64 + 10000,
            total_time 20000,
            search_term Some("rust string format"),
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            parent_domain Some("www.google.com")
        );

        // query by url
        let meta = query(&conn, "instr", 10).expect("query should work");
        assert_history_metadata_record!(&meta.metadata[0],
            url "https://www.sqlite.org/lang_corefunc.html#instr",
            title Some("Built-In Scalar SQL Functions"),
            created_at now_i64,
            updated_at now_i64 + 10000,
            total_time 20000,
            search_term Some("sqlite like"),
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            parent_domain Some("www.google.com")
        );
    }

    #[test]
    fn test_update() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        let now: Timestamp = std::time::SystemTime::now().into();
        let now_i64 = now.0 as i64;

        add_and_assert_history_metadata!(
            &conn,
            url "http://mozilla.com/1",
            title Some("Test page 1"),
            created_at now_i64,
            updated_at now_i64,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url None,
            parent_domain None
        );

        let m = get_latest_for_url(
            &conn,
            &Url::parse("http://mozilla.com/1").expect("url parse"),
        )
        .expect("get by url should work");
        let meta = m.expect("metadata record must be present");
        let db_guid = SyncGuid::new(meta.guid.expect("Record must have a guid").as_str());

        update_metadata(&conn, &db_guid, 30000).expect("update record should work");

        let updated_meta = get_latest_for_url(
            &conn,
            &Url::parse("http://mozilla.com/1").expect("url parse"),
        )
        .expect("get by url should work")
        .expect("metadata record must be present");

        assert_eq!(
            30000, updated_meta.total_view_time,
            "total view should have been updated"
        );
        assert!(
            now_i64 < updated_meta.updated_at,
            "updated_at should have been updated"
        );
    }

    #[test]
    fn test_delete_older_than() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        let now: Timestamp = std::time::SystemTime::now().into();
        let now_i64 = now.0 as i64;

        add_and_assert_history_metadata!(
            &conn,
            url "http://mozilla.com/1",
            title Some("Test page 1"),
            created_at now_i64,
            updated_at now_i64 + 10000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url None,
            parent_domain None
        );

        add_and_assert_history_metadata!(
            &conn,
            url "http://mozilla.com/2",
            title Some("Test page 2"),
            created_at now_i64,
            updated_at now_i64 + 20000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url None,
            parent_domain None
        );

        add_and_assert_history_metadata!(
            &conn,
            url "http://mozilla.com/3",
            title Some("Test page 3"),
            created_at now_i64,
            updated_at now_i64 + 30000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url None,
            parent_domain None
        );

        // deleting nothing.
        delete_older_than(&conn, (now_i64 - 40000) as i64).expect("delete worked");
        assert_eq!(
            3,
            get_since(&conn, now_i64)
                .expect("get worked")
                .metadata
                .len()
        );

        // boundary condition, should only delete the last one.
        delete_older_than(&conn, (now_i64 + 20000) as i64).expect("delete worked");
        assert_eq!(
            2,
            get_since(&conn, now_i64)
                .expect("get worked")
                .metadata
                .len()
        );
        assert_eq!(
            None,
            get_latest_for_url(&conn, &Url::parse("http://mozilla.com/1").expect("url"))
                .expect("get")
        );

        // delete everything now.
        delete_older_than(&conn, (now_i64 + 30001) as i64).expect("delete worked");
        assert_eq!(
            0,
            get_since(&conn, now_i64)
                .expect("get worked")
                .metadata
                .len()
        );
    }

    #[test]
    fn test_metadata_deletes_do_not_affect_places() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        let now: Timestamp = std::time::SystemTime::now().into();
        let now_i64 = now.0 as i64;

        add_and_assert_history_metadata!(
            &conn,
            url "https://www.mozilla.org/first/",
            title Some("Test page 0"),
            created_at now_i64,
            updated_at now_i64 + 10000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            parent_domain Some("www.google.com")
        );

        add_and_assert_history_metadata!(
            &conn,
            url "https://www.mozilla.org/",
            title Some("Test page 1"),
            created_at now_i64,
            updated_at now_i64 + 8000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            parent_domain Some("www.google.com")
        );

        // Delete all metadata.
        delete_older_than(&conn, now_i64 + 9000).expect("delete older than worked");

        // Query places. Records there should not have been affected by the delete above.
        assert_eq!(
            2,
            conn.try_query_one::<i64>("SELECT count(*) FROM moz_places", &[], true)
                .expect("select works")
                .expect("got count")
        );
    }

    #[test]
    fn test_places_delete_triggers() {
        // The cleanup functionality lives as a TRIGGER in `create_shared_triggers`.
        use crate::observation::VisitObservation;
        use crate::storage::history::{apply_observation, delete_visits_between, wipe_local};
        use crate::types::VisitTransition;

        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        let now: Timestamp = std::time::SystemTime::now().into();
        let now_i64 = now.0 as i64;
        let observation1 = VisitObservation::new(Url::parse("https://www.mozilla.org/").unwrap())
            .with_at(now)
            .with_title(Some(String::from("Test page 1")))
            .with_is_remote(false)
            .with_visit_type(VisitTransition::Link);
        let observation2 =
            VisitObservation::new(Url::parse("https://www.mozilla.org/another/").unwrap())
                .with_at(Timestamp((now_i64 + 10000) as u64))
                .with_title(Some(String::from("Test page 3")))
                .with_is_remote(false)
                .with_visit_type(VisitTransition::Link);
        let observation3 =
            VisitObservation::new(Url::parse("https://www.mozilla.org/first/").unwrap())
                .with_at(Timestamp((now_i64 - 10000) as u64))
                .with_title(Some(String::from("Test page 0")))
                .with_is_remote(true)
                .with_visit_type(VisitTransition::Link);
        apply_observation(&conn, observation1).expect("Should apply visit");
        apply_observation(&conn, observation2).expect("Should apply visit");
        apply_observation(&conn, observation3).expect("Should apply visit");

        add_and_assert_history_metadata!(
            &conn,
            url "https://www.mozilla.org/first/",
            title Some("Test page 0"),
            created_at now_i64,
            updated_at now_i64 + 10000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            parent_domain Some("www.google.com")
        );

        add_and_assert_history_metadata!(
            &conn,
            url "https://www.mozilla.org/",
            title Some("Test page 1"),
            created_at now_i64,
            updated_at now_i64 + 8000,
            total_time 20000,
            search_term None,
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            parent_domain Some("www.google.com")
        );

        add_and_assert_history_metadata!(
            &conn,
            url "https://www.mozilla.org/",
            title Some("Test page 1"),
            created_at now_i64,
            updated_at now_i64 + 9000,
            total_time 20000,
            search_term Some("mozilla"),
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            parent_domain Some("www.google.com")
        );

        add_and_assert_history_metadata!(
            &conn,
            url "https://www.mozilla.org/",
            title Some("Test page 1"),
            created_at now_i64 + 2000,
            updated_at now_i64 + 11000,
            total_time 25000,
            search_term Some("firefox"),
            is_media true,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            parent_domain Some("www.google.com")
        );

        add_and_assert_history_metadata!(
            &conn,
            url "https://www.mozilla.org/another/",
            title Some("Test page 3"),
            created_at now_i64,
            updated_at now_i64 + 10000,
            total_time 20000,
            search_term Some("mozilla"),
            is_media false,
            parent_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            parent_domain Some("www.google.com")
        );

        // double-check that we have the 'firefox' search query entry.
        assert!(conn
            .try_query_one::<i64>(
                "SELECT id FROM moz_places_metadata_search_queries WHERE term = :term",
                &[(":term", &String::from("firefox"))],
                true
            )
            .expect("select works")
            .is_some());

        // Delete our first page & its visits. Note that /another/ page will remain in place.
        delete_visits_between(
            &conn,
            Timestamp((now_i64 - 1000) as u64),
            Timestamp((now_i64 + 1000) as u64),
        )
        .expect("delete worked");

        let meta1 =
            get_latest_for_url(&conn, &Url::parse("https://www.mozilla.org/").expect("url"))
                .expect("get worked");
        let meta2 = get_latest_for_url(
            &conn,
            &Url::parse("https://www.mozilla.org/another/").expect("url"),
        )
        .expect("get worked");

        assert!(meta1.is_none(), "expected metadata to have been deleted");
        assert!(
            meta2.is_some(),
            "expected metadata to not have been deleted"
        );

        // still have a 'mozilla' search query entry, since one meta entry points to it.
        assert!(
            conn.try_query_one::<i64>(
                "SELECT id FROM moz_places_metadata_search_queries WHERE term = :term",
                &[(":term", &String::from("mozilla"))],
                true
            )
            .expect("select works")
            .is_some(),
            "search_query records with related metadata should not have been deleted"
        );

        // don't have the 'firefox' search query entry anymore, nothing points to it.
        assert!(
            conn.try_query_one::<i64>(
                "SELECT id FROM moz_places_metadata_search_queries WHERE term = :term",
                &[(":term", &String::from("firefox"))],
                true
            )
            .expect("select works")
            .is_none(),
            "search_query records without related metadata should have been deleted"
        );

        // now, let's wipe places, and make sure none of the metadata stuff remains.
        wipe_local(&conn).expect("places wipe succeeds");

        let all_metadata = conn
            .query_rows_and_then_named_cached(
                COMMON_METADATA_SELECT,
                rusqlite::named_params! {},
                HistoryMetadata::from_row,
            )
            .expect("select all metadata worked");

        assert_eq!(0, all_metadata.len());

        assert_eq!(
            0,
            conn.try_query_one::<i64>(
                "SELECT count(*) FROM moz_places_metadata_search_queries",
                &[],
                true
            )
            .expect("select works")
            .expect("got count")
        );
    }
}

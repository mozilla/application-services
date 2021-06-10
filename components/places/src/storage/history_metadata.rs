/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::{PlacesDb, PlacesTransaction};
use crate::error::Result;
use crate::msg_types::{HistoryMetadata, HistoryMetadataList, HistoryMetadataObservation};
use sql_support::{self, ConnExt};
use sync_guid::Guid as SyncGuid;
use types::Timestamp;
use url::Url;

use lazy_static::lazy_static;

enum PlaceEntry {
    Existing(i64),
    CreateFor(Url, Option<String>),
}

impl PlaceEntry {
    fn fetch(url: &str, tx: &PlacesTransaction<'_>, title: Option<String>) -> Result<Self> {
        let url = Url::parse(url)?;
        let place_id = tx.try_query_one(
            "SELECT id FROM moz_places WHERE url_hash = hash(:url) AND url = :url",
            &[(":url", &url.as_str())],
            true,
        )?;

        Ok(match place_id {
            Some(id) => PlaceEntry::Existing(id),
            None => PlaceEntry::CreateFor(url, title),
        })
    }
}

trait DatabaseId {
    fn get_or_insert(&self, tx: &PlacesTransaction<'_>) -> Result<i64>;
}

impl DatabaseId for PlaceEntry {
    fn get_or_insert(&self, tx: &PlacesTransaction<'_>) -> Result<i64> {
        Ok(match self {
            PlaceEntry::Existing(id) => *id,
            PlaceEntry::CreateFor(url, title) => {
                let sql = "INSERT INTO moz_places (guid, url, title, url_hash)
                VALUES (:guid, :url, :title, hash(:url))";

                let guid = SyncGuid::random();

                tx.execute_named_cached(
                    sql,
                    &[
                        (":guid", &guid),
                        (":title", &title),
                        (":url", &url.as_str()),
                    ],
                )?;
                tx.conn().last_insert_rowid()
            }
        })
    }
}

enum SearchQueryEntry {
    Existing(i64),
    CreateFor(String),
}

impl DatabaseId for SearchQueryEntry {
    fn get_or_insert(&self, tx: &PlacesTransaction<'_>) -> Result<i64> {
        Ok(match self {
            SearchQueryEntry::Existing(id) => *id,
            SearchQueryEntry::CreateFor(term) => {
                tx.execute_named_cached(
                    "INSERT INTO moz_places_metadata_search_queries(term) VALUES (:term)",
                    &[(":term", &term)],
                )?;
                tx.conn().last_insert_rowid()
            }
        })
    }
}

impl SearchQueryEntry {
    fn from(search_term: &str, tx: &PlacesTransaction<'_>) -> Result<Self> {
        let lowercase_term = search_term.to_lowercase();
        Ok(
            match tx.try_query_one(
                "SELECT id FROM moz_places_metadata_search_queries WHERE term = :term",
                &[(":term", &lowercase_term)],
                true,
            )? {
                Some(id) => SearchQueryEntry::Existing(id),
                None => SearchQueryEntry::CreateFor(lowercase_term),
            },
        )
    }
}

struct HistoryMetadataCompoundKey {
    place_entry: PlaceEntry,
    referrer_entry: Option<PlaceEntry>,
    search_query_entry: Option<SearchQueryEntry>,
}

struct MetadataObservation {
    document_type: Option<i32>,
    view_time: Option<i32>,
}

impl HistoryMetadataCompoundKey {
    fn can_debounce(&self) -> Option<i64> {
        match self.place_entry {
            PlaceEntry::Existing(id) => {
                if (match self.search_query_entry {
                    None | Some(SearchQueryEntry::Existing(_)) => true,
                    Some(SearchQueryEntry::CreateFor(_)) => false,
                } && match self.referrer_entry {
                    None | Some(PlaceEntry::Existing(_)) => true,
                    Some(PlaceEntry::CreateFor(_, _)) => false,
                }) {
                    Some(id)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    // Looks up matching metadata records, by the compound key and time window.
    fn lookup(&self, tx: &PlacesTransaction<'_>, newer_than: i64) -> Result<Option<i64>> {
        Ok(match self.can_debounce() {
            Some(id) => {
                let search_query_id = match self.search_query_entry {
                    None | Some(SearchQueryEntry::CreateFor(_)) => None,
                    Some(SearchQueryEntry::Existing(id)) => Some(id),
                };

                let referrer_place_id = match self.referrer_entry {
                    None | Some(PlaceEntry::CreateFor(_, _)) => None,
                    Some(PlaceEntry::Existing(id)) => Some(id),
                };

                tx.try_query_one::<i64>(
                    "SELECT id FROM moz_places_metadata
                        WHERE
                            place_id IS :place_id AND
                            referrer_place_id IS :referrer_place_id AND
                            search_query_id IS :search_query_id AND
                            updated_at >= :newer_than
                        ORDER BY updated_at DESC LIMIT 1",
                    rusqlite::named_params! {
                        ":place_id": id,
                        ":search_query_id": search_query_id,
                        ":referrer_place_id": referrer_place_id,
                        ":newer_than": newer_than
                    },
                    true,
                )?
            }
            None => None,
        })
    }
}

const DEBOUNCE_WINDOW_MS: i64 = 2 * 60 * 1000; // 2 minutes
const MAX_QUERY_RESULTS: i32 = 1000;

const COMMON_METADATA_SELECT: &str = "
SELECT
    m.id as metadata_id, p.url as url, p.title as title, m.created_at as created_at,
    m.updated_at as updated_at, m.total_view_time as total_view_time,
    m.document_type as document_type, o.url as referrer_url, s.term as search_term
FROM moz_places_metadata m
LEFT JOIN moz_places p ON m.place_id = p.id
LEFT JOIN moz_places_metadata_search_queries s ON m.search_query_id = s.id
LEFT JOIN moz_places o ON o.id = m.referrer_place_id";

lazy_static! {
    static ref GET_LATEST_SQL: String = format!(
        "{common_select_sql}
        WHERE p.url_hash = hash(:url) AND p.url = :url
        ORDER BY updated_at DESC, metadata_id DESC
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
            p.url LIKE :query OR
            p.title LIKE :query OR
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

pub fn delete_older_than(db: &PlacesDb, older_than: i64) -> Result<()> {
    db.execute_named_cached(
        "DELETE FROM moz_places_metadata
         WHERE updated_at < :older_than",
        &[(":older_than", &older_than)],
    )?;
    Ok(())
}

pub fn apply_metadata_observation(
    db: &PlacesDb,
    observation: HistoryMetadataObservation,
) -> Result<()> {
    // Begin a write transaction. We do this before any other work (e.g. SELECTs) to avoid racing against
    // other writers. Even though we expect to only have a single application writer, a sync writer
    // can come in at any time and change data we depend on, such as moz_places
    // and moz_origins, leaving us in a potentially inconsistent state.
    let tx = db.begin_transaction()?;

    let place_entry = PlaceEntry::fetch(&observation.url, &tx, observation.title.clone())?;
    let result = apply_metadata_observation_impl(&tx, place_entry, observation);

    // Inserting into moz_places has side-effects (temp tables are populated via triggers and need to be flushed).
    // This call "finalizes" these side-effects.
    super::delete_pending_temp_tables(db)?;
    match result {
        Ok(_) => tx.commit()?,
        Err(_) => tx.rollback()?,
    };

    result
}

fn apply_metadata_observation_impl(
    tx: &PlacesTransaction<'_>,
    place_entry: PlaceEntry,
    observation: HistoryMetadataObservation,
) -> Result<()> {
    let referrer_entry = match observation.referrer_url {
        Some(referrer_url) if !referrer_url.is_empty() => {
            Some(PlaceEntry::fetch(&referrer_url, &tx, None)?)
        }
        Some(_) | None => None,
    };
    let search_query_entry = match observation.search_term {
        Some(search_term) if !search_term.is_empty() => {
            Some(SearchQueryEntry::from(&search_term, &tx)?)
        }
        Some(_) | None => None,
    };

    let compound_key = HistoryMetadataCompoundKey {
        place_entry,
        referrer_entry,
        search_query_entry,
    };

    let observation = MetadataObservation {
        document_type: observation.document_type,
        view_time: observation.view_time,
    };

    let now = Timestamp::now().as_millis() as i64;
    let newer_than = now - DEBOUNCE_WINDOW_MS;
    let matching_metadata = compound_key.lookup(&tx, newer_than)?;

    // If a matching record exists, update it; otherwise, insert a new one.
    match matching_metadata {
        Some(metadata_id) => {
            // If document_type isn't part of the observation, make sure we don't accidentally erase what's currently set.
            match observation {
                MetadataObservation {
                    document_type: Some(dt),
                    view_time,
                } => {
                    tx.execute_named_cached(
                        "UPDATE
                            moz_places_metadata
                        SET
                            document_type = :document_type,
                            total_view_time = total_view_time + :view_time_delta,
                            updated_at = :updated_at
                        WHERE id = :id",
                        rusqlite::named_params! {
                            ":id": metadata_id,
                            ":document_type": dt,
                            ":view_time_delta": view_time.unwrap_or(0),
                            ":updated_at": now
                        },
                    )?;
                }
                MetadataObservation {
                    document_type: None,
                    view_time,
                } => {
                    tx.execute_named_cached(
                        "UPDATE
                            moz_places_metadata
                        SET
                            total_view_time = total_view_time + :view_time_delta,
                            updated_at = :updated_at
                        WHERE id = :id",
                        rusqlite::named_params! {
                            ":id": metadata_id,
                            ":view_time_delta": view_time.unwrap_or(0),
                            ":updated_at": now
                        },
                    )?;
                }
            }
            Ok(())
        }
        None => insert_metadata_in_tx(&tx, compound_key, observation),
    }
}

fn insert_metadata_in_tx(
    tx: &PlacesTransaction<'_>,
    key: HistoryMetadataCompoundKey,
    observation: MetadataObservation,
) -> Result<()> {
    let now = Timestamp::now();

    let referrer_place_id = match key.referrer_entry {
        None => None,
        Some(entry) => Some(entry.get_or_insert(&tx)?),
    };

    let search_query_id = match key.search_query_entry {
        None => None,
        Some(entry) => Some(entry.get_or_insert(&tx)?),
    };

    // Heavy lifting around moz_places inserting (e.g. updating moz_origins, frecency, etc) is performed via triggers.
    // This lets us simply INSERT here without worrying about the rest.
    let place_id = key.place_entry.get_or_insert(&tx)?;

    let sql = "INSERT INTO moz_places_metadata
        (place_id, created_at, updated_at, total_view_time, search_query_id, document_type, referrer_place_id)
    VALUES
        (:place_id, :created_at, :updated_at, :total_view_time, :search_query_id, :document_type, :referrer_place_id)";

    tx.execute_named_cached(
        sql,
        &[
            (":place_id", &place_id),
            (":created_at", &now),
            (":updated_at", &now),
            (":search_query_id", &search_query_id),
            (":referrer_place_id", &referrer_place_id),
            (":document_type", &observation.document_type.unwrap_or(0)),
            (":total_view_time", &observation.view_time.unwrap_or(0)),
        ],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::ConnectionType;
    use pretty_assertions::assert_eq;
    use std::{thread, time};

    macro_rules! assert_table_size {
        ($conn:expr, $table:expr, $count:expr) => {
            assert_eq!(
                $count,
                $conn
                    .try_query_one::<i64>(
                        format!("SELECT count(*) FROM {table}", table = $table).as_str(),
                        &[],
                        true
                    )
                    .expect("select works")
                    .expect("got count")
            );
        };
    }

    #[macro_use]
    macro_rules! assert_history_metadata_record {
        ($record:expr, url $url:expr, total_time $tvt:expr, search_term $search_term:expr, document_type $document_type:expr, referrer_url $referrer_url:expr, title $title:expr) => {
            assert_eq!(String::from($url), $record.url, "url must match");
            assert_eq!($tvt, $record.total_view_time, "total_view_time must match");
            assert_eq!($document_type, $record.document_type, "is_media must match");

            let meta = $record.clone(); // ugh... not sure why this `clone` is necessary.

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
            match $referrer_url as Option<&str> {
                Some(t) => assert_eq!(
                    String::from(t),
                    meta.referrer_url.expect("referrer_url must be Some"),
                    "referrer_url must match"
                ),
                None => assert_eq!(
                    true,
                    meta.referrer_url.is_none(),
                    "referrer_url expected to be None"
                ),
            };
            match $title as Option<&str> {
                Some(t) => assert_eq!(
                    String::from(t),
                    meta.title.expect("title must be Some"),
                    "title must match"
                ),
                None => assert_eq!(true, meta.title.is_none(), "title expected to be None"),
            };
        };
    }

    #[macro_use]
    macro_rules! assert_total_after_observation {
        ($conn:expr, total_records_after $total_records:expr, total_view_time_after $total_view_time:expr, url $url:expr, view_time $view_time:expr, search_term $search_term:expr, document_type $document_type:expr, referrer_url $referrer_url:expr, title $title:expr) => {
            note_observation!($conn,
                url $url,
                view_time $view_time,
                search_term $search_term,
                document_type $document_type,
                referrer_url $referrer_url,
                title $title
            );

            assert_table_size!($conn, "moz_places_metadata", $total_records);
            let updated = get_latest_for_url($conn, &Url::parse($url).unwrap()).unwrap().unwrap();
            assert_eq!($total_view_time, updated.total_view_time, "total view time must match");
        }
    }

    #[macro_use]
    macro_rules! note_observation {
        ($conn:expr, url $url:expr, view_time $view_time:expr, search_term $search_term:expr, document_type $document_type:expr, referrer_url $referrer_url:expr, title $title:expr) => {
            apply_metadata_observation(
                $conn,
                HistoryMetadataObservation {
                    url: String::from($url),
                    view_time: $view_time,
                    search_term: $search_term.map(|s: &str| s.to_string()),
                    document_type: $document_type,
                    referrer_url: $referrer_url.map(|s: &str| s.to_string()),
                    title: $title.map(|s: &str| s.to_string()),
                },
            )
            .unwrap();
        };
    }

    #[macro_use]
    macro_rules! assert_after_observation {
        ($conn:expr, total_records_after $total_records:expr, total_view_time_after $total_view_time:expr, url $url:expr, view_time $view_time:expr, search_term $search_term:expr, document_type $document_type:expr, referrer_url $referrer_url:expr, title $title:expr, assertion $assertion:expr) => {
            // can set title on creating a new record
            assert_total_after_observation!($conn,
                total_records_after $total_records,
                total_view_time_after $total_view_time,
                url $url,
                view_time $view_time,
                search_term $search_term,
                document_type $document_type,
                referrer_url $referrer_url,
                title $title
            );

            let m = get_latest_for_url(
                $conn,
                &Url::parse(&String::from($url)).unwrap(),
            )
            .unwrap()
            .unwrap();
            $assertion(m);
        }
    }

    #[test]
    fn test_note_observation() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).unwrap();

        assert_table_size!(&conn, "moz_places_metadata", 0);

        assert_total_after_observation!(&conn,
            total_records_after 1,
            total_view_time_after 1500,
            url "http://mozilla.com/",
            view_time Some(1500),
            search_term None,
            document_type Some(0),
            referrer_url None,
            title None
        );

        // debounced! total time was updated
        assert_total_after_observation!(&conn,
            total_records_after 1,
            total_view_time_after 2500,
            url "http://mozilla.com/",
            view_time Some(1000),
            search_term None,
            document_type Some(0),
            referrer_url None,
            title None
        );

        // different document type, record updated
        assert_total_after_observation!(&conn,
            total_records_after 1,
            total_view_time_after 3500,
            url "http://mozilla.com/",
            view_time Some(1000),
            search_term None,
            document_type Some(1),
            referrer_url None,
            title None
        );

        // referrer set
        assert_total_after_observation!(&conn,
            total_records_after 2,
            total_view_time_after 2000,
            url "http://mozilla.com/",
            view_time Some(2000),
            search_term None,
            document_type Some(1),
            referrer_url Some("https://news.website"),
            title None
        );

        // search term and referrer are set
        assert_total_after_observation!(&conn,
            total_records_after 3,
            total_view_time_after 1100,
            url "http://mozilla.com/",
            view_time Some(1100),
            search_term Some("firefox"),
            document_type Some(1),
            referrer_url Some("https://www.google.com/search?client=firefox-b-d&q=firefox"),
            title None
        );

        // debounce!
        assert_total_after_observation!(&conn,
            total_records_after 3,
            total_view_time_after 6100,
            url "http://mozilla.com/",
            view_time Some(5000),
            search_term Some("firefox"),
            document_type Some(1),
            referrer_url Some("https://www.google.com/search?client=firefox-b-d&q=firefox"),
            title None
        );

        // different url now
        assert_total_after_observation!(&conn,
            total_records_after 4,
            total_view_time_after 3000,
            url "http://mozilla.com/another",
            view_time Some(3000),
            search_term None,
            document_type Some(0),
            referrer_url Some("https://news.website/tech"),
            title None
        );

        // shared origin for both url and referrer
        assert_total_after_observation!(&conn,
            total_records_after 5,
            total_view_time_after 100000,
            url "https://www.youtube.com/watch?v=tpiyEe_CqB4",
            view_time Some(100000),
            search_term Some("cute cat"),
            document_type Some(1),
            referrer_url Some("https://www.youtube.com/results?search_query=cute+cat"),
            title None
        );

        // empty search term/referrer url are treated the same as None
        assert_total_after_observation!(&conn,
            total_records_after 6,
            total_view_time_after 80000,
            url "https://www.youtube.com/watch?v=daff43jif3",
            view_time Some(80000),
            search_term Some(""),
            document_type Some(1),
            referrer_url Some(""),
            title None
        );

        assert_total_after_observation!(&conn,
            total_records_after 6,
            total_view_time_after 90000,
            url "https://www.youtube.com/watch?v=daff43jif3",
            view_time Some(10000),
            search_term None,
            document_type Some(1),
            referrer_url None,
            title None
        );

        // document type recording
        assert_total_after_observation!(&conn,
            total_records_after 7,
            total_view_time_after 0,
            url "https://www.youtube.com/watch?v=fds32fds",
            view_time None,
            search_term None,
            document_type Some(1),
            referrer_url None,
            title None
        );

        // now, update the view time as a separate call
        assert_total_after_observation!(&conn,
            total_records_after 7,
            total_view_time_after 1338,
            url "https://www.youtube.com/watch?v=fds32fds",
            view_time Some(1338),
            search_term None,
            document_type None,
            referrer_url None,
            title None
        );

        // and again, bump the view time
        assert_total_after_observation!(&conn,
            total_records_after 7,
            total_view_time_after 2000,
            url "https://www.youtube.com/watch?v=fds32fds",
            view_time Some(662),
            search_term None,
            document_type None,
            referrer_url None,
            title None
        );

        // now try the other way - record view time first, document type after.
        // and again, bump the view time
        assert_after_observation!(&conn,
            total_records_after 8,
            total_view_time_after 662,
            url "https://www.youtube.com/watch?v=dasdg34d",
            view_time Some(662),
            search_term None,
            document_type None,
            referrer_url None,
            title None,
            assertion |m: HistoryMetadata| { assert_eq!(0, m.document_type) }
        );

        assert_after_observation!(&conn,
            total_records_after 8,
            total_view_time_after 662,
            url "https://www.youtube.com/watch?v=dasdg34d",
            view_time None,
            search_term None,
            document_type Some(1),
            referrer_url None,
            title None,
            assertion |m: HistoryMetadata| { assert_eq!(1, m.document_type) }
        );

        // document type not overwritten (e.g. remains 1, not default 0).
        assert_after_observation!(&conn,
            total_records_after 8,
            total_view_time_after 675,
            url "https://www.youtube.com/watch?v=dasdg34d",
            view_time Some(13),
            search_term None,
            document_type None,
            referrer_url None,
            title None,
            assertion |m: HistoryMetadata| { assert_eq!(1, m.document_type) }
        );

        // can set title on creating a new record
        assert_after_observation!(&conn,
            total_records_after 9,
            total_view_time_after 13,
            url "https://www.youtube.com/watch?v=dasdsada",
            view_time Some(13),
            search_term None,
            document_type None,
            referrer_url None,
            title Some("hello!"),
            assertion |m: HistoryMetadata| { assert_eq!(Some(String::from("hello!")), m.title) }
        );

        // can not update title after
        assert_after_observation!(&conn,
            total_records_after 9,
            total_view_time_after 26,
            url "https://www.youtube.com/watch?v=dasdsada",
            view_time Some(13),
            search_term None,
            document_type None,
            referrer_url None,
            title Some("world!"),
            assertion |m: HistoryMetadata| { assert_eq!(Some(String::from("hello!")), m.title) }
        );
    }

    #[test]
    fn test_get_between() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        assert_eq!(0, get_between(&conn, 0, 0).unwrap().metadata.len());

        let beginning = Timestamp::now().as_millis() as i64;
        note_observation!(&conn,
            url "http://mozilla.com/another",
            view_time Some(3000),
            search_term None,
            document_type Some(0),
            referrer_url Some("https://news.website/tech"),
            title None
        );
        let after_meta1 = Timestamp::now().as_millis() as i64;

        assert_eq!(
            0,
            get_between(&conn, 0, beginning - 1).unwrap().metadata.len()
        );
        assert_eq!(
            1,
            get_between(&conn, 0, after_meta1).unwrap().metadata.len()
        );

        thread::sleep(time::Duration::from_millis(10));

        note_observation!(&conn,
            url "http://mozilla.com/video/",
            view_time Some(1000),
            search_term None,
            document_type Some(1),
            referrer_url None,
            title None
        );
        let after_meta2 = Timestamp::now().as_millis() as i64;

        assert_eq!(
            1,
            get_between(&conn, beginning, after_meta1)
                .unwrap()
                .metadata
                .len()
        );
        assert_eq!(
            2,
            get_between(&conn, beginning, after_meta2)
                .unwrap()
                .metadata
                .len()
        );
        assert_eq!(
            1,
            get_between(&conn, after_meta1, after_meta2)
                .unwrap()
                .metadata
                .len()
        );
        assert_eq!(
            0,
            get_between(&conn, after_meta2, after_meta2 + 1)
                .unwrap()
                .metadata
                .len()
        );
    }

    #[test]
    fn test_get_since() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        assert_eq!(0, get_since(&conn, 0).unwrap().metadata.len());

        let beginning = Timestamp::now().as_millis() as i64;
        note_observation!(&conn,
            url "http://mozilla.com/another",
            view_time Some(3000),
            search_term None,
            document_type Some(0),
            referrer_url Some("https://news.website/tech"),
            title None
        );
        let after_meta1 = Timestamp::now().as_millis() as i64;

        assert_eq!(1, get_since(&conn, 0).unwrap().metadata.len());
        assert_eq!(1, get_since(&conn, beginning).unwrap().metadata.len());
        assert_eq!(0, get_since(&conn, after_meta1).unwrap().metadata.len());

        // thread::sleep(time::Duration::from_millis(50));

        note_observation!(&conn,
            url "http://mozilla.com/video/",
            view_time Some(1000),
            search_term None,
            document_type Some(1),
            referrer_url None,
            title None
        );
        let after_meta2 = Timestamp::now().as_millis() as i64;
        assert_eq!(2, get_since(&conn, beginning).unwrap().metadata.len());
        assert_eq!(1, get_since(&conn, after_meta1).unwrap().metadata.len());
        assert_eq!(0, get_since(&conn, after_meta2).unwrap().metadata.len());
    }

    #[test]
    fn test_query() {
        use crate::observation::VisitObservation;
        use crate::storage::history::apply_observation;
        use crate::types::VisitTransition;

        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");
        let now = Timestamp::now();

        // need a history observation to get a title query working.
        let observation1 = VisitObservation::new(Url::parse("https://www.cbc.ca/news/politics/federal-budget-2021-freeland-zimonjic-1.5991021").unwrap())
                .with_at(now)
                .with_title(Some(String::from("Budget vows to build &#x27;for the long term&#x27; as it promises child care cash, projects massive deficits | CBC News")))
                .with_is_remote(false)
                .with_visit_type(VisitTransition::Link);
        apply_observation(&conn, observation1).unwrap();

        note_observation!(
            &conn,
            url "https://www.cbc.ca/news/politics/federal-budget-2021-freeland-zimonjic-1.5991021",
            view_time Some(20000),
            search_term Some("cbc federal budget 2021"),
            document_type Some(0),
            referrer_url Some("https://yandex.ru/search/?text=cbc%20federal%20budget%202021&lr=21512"),
            title None
        );

        note_observation!(
            &conn,
            url "https://stackoverflow.com/questions/37777675/how-to-create-a-formatted-string-out-of-a-literal-in-rust",
            view_time Some(20000),
            search_term Some("rust string format"),
            document_type Some(0),
            referrer_url Some("https://yandex.ru/search/?lr=21512&text=rust%20string%20format"),
            title None
        );

        note_observation!(
            &conn,
            url "https://www.sqlite.org/lang_corefunc.html#instr",
            view_time Some(20000),
            search_term Some("sqlite like"),
            document_type Some(0),
            referrer_url Some("https://www.google.com/search?client=firefox-b-d&q=sqlite+like"),
            title None
        );

        note_observation!(
            &conn,
            url "https://www.youtube.com/watch?v=tpiyEe_CqB4",
            view_time Some(100000),
            search_term Some("cute cat"),
            document_type Some(1),
            referrer_url Some("https://www.youtube.com/results?search_query=cute+cat"),
            title None
        );

        // query by title
        let meta = query(&conn, "child care", 10).expect("query should work");
        assert_eq!(1, meta.metadata.len(), "expected exactly one result");
        assert_history_metadata_record!(&meta.metadata[0],
            url "https://www.cbc.ca/news/politics/federal-budget-2021-freeland-zimonjic-1.5991021",
            total_time 20000,
            search_term Some("cbc federal budget 2021"),
            document_type 0,
            referrer_url Some("https://yandex.ru/search/?text=cbc%20federal%20budget%202021&lr=21512"),
            title Some("Budget vows to build &#x27;for the long term&#x27; as it promises child care cash, projects massive deficits | CBC News")
        );

        // query by search term
        let meta = query(&conn, "string format", 10).expect("query should work");
        assert_eq!(1, meta.metadata.len(), "expected exactly one result");
        assert_history_metadata_record!(&meta.metadata[0],
            url "https://stackoverflow.com/questions/37777675/how-to-create-a-formatted-string-out-of-a-literal-in-rust",
            total_time 20000,
            search_term Some("rust string format"),
            document_type 0,
            referrer_url Some("https://yandex.ru/search/?lr=21512&text=rust%20string%20format"),
            title None
        );

        // query by url
        let meta = query(&conn, "instr", 10).expect("query should work");
        assert_history_metadata_record!(&meta.metadata[0],
            url "https://www.sqlite.org/lang_corefunc.html#instr",
            total_time 20000,
            search_term Some("sqlite like"),
            document_type 0,
            referrer_url Some("https://www.google.com/search?client=firefox-b-d&q=sqlite+like"),
            title None
        );

        // by url, referrer domain is different
        let meta = query(&conn, "youtube", 10).expect("query should work");
        assert_history_metadata_record!(&meta.metadata[0],
            url "https://www.youtube.com/watch?v=tpiyEe_CqB4",
            total_time 100000,
            search_term Some("cute cat"),
            document_type 1,
            referrer_url Some("https://www.youtube.com/results?search_query=cute+cat"),
            title None
        );
    }

    #[test]
    fn test_delete_older_than() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        let beginning = Timestamp::now().as_millis() as i64;

        note_observation!(&conn,
            url "http://mozilla.com/1",
            view_time Some(20000),
            search_term None,
            document_type Some(0),
            referrer_url None,
            title None
        );
        let after_meta1 = Timestamp::now().as_millis() as i64;

        thread::sleep(time::Duration::from_millis(10));

        note_observation!(&conn,
            url "http://mozilla.com/2",
            view_time Some(20000),
            search_term None,
            document_type Some(0),
            referrer_url None,
            title None
        );

        thread::sleep(time::Duration::from_millis(10));

        note_observation!(&conn,
            url "http://mozilla.com/3",
            view_time Some(20000),
            search_term None,
            document_type Some(0),
            referrer_url None,
            title None
        );
        let after_meta3 = Timestamp::now().as_millis() as i64;

        // deleting nothing.
        delete_older_than(&conn, beginning).expect("delete worked");
        assert_eq!(
            3,
            get_since(&conn, beginning)
                .expect("get worked")
                .metadata
                .len()
        );

        // boundary condition, should only delete the last one.
        delete_older_than(&conn, after_meta1).expect("delete worked");
        assert_eq!(
            2,
            get_since(&conn, beginning)
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
        delete_older_than(&conn, after_meta3).expect("delete worked");
        assert_eq!(
            0,
            get_since(&conn, beginning)
                .expect("get worked")
                .metadata
                .len()
        );
    }

    #[test]
    fn test_metadata_deletes_do_not_affect_places() {
        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        note_observation!(
            &conn,
            url "https://www.mozilla.org/first/",
            view_time Some(20000),
            search_term None,
            document_type Some(0),
            referrer_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            title None
        );

        note_observation!(
            &conn,
            url "https://www.mozilla.org/",
            view_time Some(20000),
            search_term None,
            document_type Some(0),
            referrer_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            title None
        );
        let after_meta_added = Timestamp::now().as_millis() as i64;

        // Delete all metadata.
        delete_older_than(&conn, after_meta_added).expect("delete older than worked");

        // Query places. Records there should not have been affected by the delete above.
        // 2 for metadata entries + 1 for referrer url.
        assert_table_size!(&conn, "moz_places", 3);
    }

    #[test]
    fn test_places_delete_triggers_with_bookmarks() {
        use crate::storage::bookmarks::{
            self, BookmarkPosition, BookmarkRootGuid, InsertableBookmark, InsertableItem,
        };

        // The cleanup functionality lives as a TRIGGER in `create_shared_triggers`.
        use crate::observation::VisitObservation;
        use crate::storage::history::{apply_observation, wipe_local};
        use crate::types::VisitTransition;

        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        let now = Timestamp::now();
        let url = Url::parse("https://www.mozilla.org/").unwrap();
        let parent_url =
            Url::parse("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox")
                .unwrap();

        let observation1 = VisitObservation::new(url.clone())
            .with_at(now)
            .with_title(Some(String::from("Test page 0")))
            .with_is_remote(false)
            .with_visit_type(VisitTransition::Link);

        let observation2 = VisitObservation::new(parent_url.clone())
            .with_at(now)
            .with_title(Some(String::from("Test page 1")))
            .with_is_remote(false)
            .with_visit_type(VisitTransition::Link);

        apply_observation(&conn, observation1).expect("Should apply visit");
        apply_observation(&conn, observation2).expect("Should apply visit");

        assert_table_size!(&conn, "moz_bookmarks", 5);

        // add bookmark for the page we have a metadata entry
        bookmarks::insert_bookmark(
            &conn,
            &InsertableItem::Bookmark(InsertableBookmark {
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: BookmarkPosition::Append,
                date_added: None,
                last_modified: None,
                guid: Some(SyncGuid::from("cccccccccccc")),
                url,
                title: None,
            }),
        )
        .expect("bookmark insert worked");

        // add another bookmark to the "parent" of our metadata entry
        bookmarks::insert_bookmark(
            &conn,
            &InsertableItem::Bookmark(InsertableBookmark {
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: BookmarkPosition::Append,
                date_added: None,
                last_modified: None,
                guid: Some(SyncGuid::from("ccccccccccca")),
                url: parent_url,
                title: None,
            }),
        )
        .expect("bookmark insert worked");

        assert_table_size!(&conn, "moz_bookmarks", 7);
        assert_table_size!(&conn, "moz_origins", 2);

        note_observation!(
            &conn,
            url "https://www.mozilla.org/",
            view_time Some(20000),
            search_term Some("mozilla firefox"),
            document_type Some(0),
            referrer_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            title None
        );

        assert_table_size!(&conn, "moz_origins", 2);

        // this somehow deletes 1 origin record, and our metadata
        wipe_local(&conn).expect("places wipe succeeds");

        assert_table_size!(&conn, "moz_places_metadata", 0);
        assert_table_size!(&conn, "moz_places_metadata_search_queries", 0);
    }

    #[test]
    fn test_places_delete_triggers() {
        // The cleanup functionality lives as a TRIGGER in `create_shared_triggers`.
        use crate::observation::VisitObservation;
        use crate::storage::history::{apply_observation, delete_visits_between, wipe_local};
        use crate::types::VisitTransition;

        let conn = PlacesDb::open_in_memory(ConnectionType::ReadWrite).expect("memory db");

        let now = Timestamp::now();
        let observation1 = VisitObservation::new(Url::parse("https://www.mozilla.org/").unwrap())
            .with_at(now)
            .with_title(Some(String::from("Test page 1")))
            .with_is_remote(false)
            .with_visit_type(VisitTransition::Link);
        let observation2 =
            VisitObservation::new(Url::parse("https://www.mozilla.org/another/").unwrap())
                .with_at(Timestamp((now.as_millis() + 10000) as u64))
                .with_title(Some(String::from("Test page 3")))
                .with_is_remote(false)
                .with_visit_type(VisitTransition::Link);
        let observation3 =
            VisitObservation::new(Url::parse("https://www.mozilla.org/first/").unwrap())
                .with_at(Timestamp((now.as_millis() - 10000) as u64))
                .with_title(Some(String::from("Test page 0")))
                .with_is_remote(true)
                .with_visit_type(VisitTransition::Link);
        apply_observation(&conn, observation1).expect("Should apply visit");
        apply_observation(&conn, observation2).expect("Should apply visit");
        apply_observation(&conn, observation3).expect("Should apply visit");

        note_observation!(
            &conn,
            url "https://www.mozilla.org/first/",
            view_time Some(20000),
            search_term None,
            document_type Some(0),
            referrer_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            title None
        );

        note_observation!(
            &conn,
            url "https://www.mozilla.org/",
            view_time Some(20000),
            search_term None,
            document_type Some(0),
            referrer_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            title None
        );

        note_observation!(
            &conn,
            url "https://www.mozilla.org/",
            view_time Some(20000),
            search_term Some("mozilla"),
            document_type Some(0),
            referrer_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            title None
        );

        note_observation!(
            &conn,
            url "https://www.mozilla.org/",
            view_time Some(25000),
            search_term Some("firefox"),
            document_type Some(1),
            referrer_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            title None
        );

        note_observation!(
            &conn,
            url "https://www.mozilla.org/another/",
            view_time Some(20000),
            search_term Some("mozilla"),
            document_type Some(0),
            referrer_url Some("https://www.google.com/search?client=firefox-b-d&q=mozilla+firefox"),
            title None
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
            Timestamp((now.as_millis() - 1000) as u64),
            Timestamp((now.as_millis() + 1000) as u64),
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

        assert_table_size!(&conn, "moz_places_metadata", 0);
        assert_table_size!(&conn, "moz_places_metadata_search_queries", 0);
    }
}

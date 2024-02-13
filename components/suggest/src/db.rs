/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use std::{collections::HashSet, path::Path, sync::Arc};

use interrupt_support::{SqlInterruptHandle, SqlInterruptScope};
use parking_lot::Mutex;
use remote_settings::RemoteSettingsRecord;
use rusqlite::{
    named_params,
    types::{FromSql, ToSql},
    Connection, OpenFlags,
};
use sql_support::{open_database::open_database_with_flags, ConnExt};

use crate::{
    config::{SuggestGlobalConfig, SuggestProviderConfig},
    keyword::full_keyword,
    pocket::{split_keyword, KeywordConfidence},
    provider::SuggestionProvider,
    rs::{
        DownloadedAmoSuggestion, DownloadedAmpSuggestion, DownloadedAmpWikipediaSuggestion,
        DownloadedMdnSuggestion, DownloadedPocketSuggestion, DownloadedWeatherData,
        SuggestRecordId,
    },
    schema::{SuggestConnectionInitializer, VERSION},
    store::{UnparsableRecord, UnparsableRecords},
    suggestion::{cook_raw_suggestion_url, AmpSuggestionType, Suggestion},
    Result, SuggestionQuery,
};

/// The metadata key whose value is the timestamp of the last record ingested
/// from the Suggest Remote Settings collection.
pub const LAST_INGEST_META_KEY: &str = "last_quicksuggest_ingest";
/// The metadata key whose value keeps track of records of suggestions
/// that aren't parsable and which schema version it was first seen in.
pub const UNPARSABLE_RECORDS_META_KEY: &str = "unparsable_records";
/// The metadata key whose value is a JSON string encoding a
/// `SuggestGlobalConfig`, which contains global Suggest configuration data.
pub const GLOBAL_CONFIG_META_KEY: &str = "global_config";
/// Prefix of metadata keys whose values are JSON strings encoding
/// `SuggestProviderConfig`, which contains per-provider configuration data. The
/// full key is this prefix plus the `SuggestionProvider` value as a u8.
pub const PROVIDER_CONFIG_META_KEY_PREFIX: &str = "provider_config_";

// Default value when Suggestion does not have a value for score
pub const DEFAULT_SUGGESTION_SCORE: f64 = 0.2;

/// The database connection type.
#[derive(Clone, Copy)]
pub(crate) enum ConnectionType {
    ReadOnly,
    ReadWrite,
}

impl From<ConnectionType> for OpenFlags {
    fn from(type_: ConnectionType) -> Self {
        match type_ {
            ConnectionType::ReadOnly => {
                OpenFlags::SQLITE_OPEN_URI
                    | OpenFlags::SQLITE_OPEN_NO_MUTEX
                    | OpenFlags::SQLITE_OPEN_READ_ONLY
            }
            ConnectionType::ReadWrite => {
                OpenFlags::SQLITE_OPEN_URI
                    | OpenFlags::SQLITE_OPEN_NO_MUTEX
                    | OpenFlags::SQLITE_OPEN_CREATE
                    | OpenFlags::SQLITE_OPEN_READ_WRITE
            }
        }
    }
}

/// A thread-safe wrapper around an SQLite connection to the Suggest database,
/// and its interrupt handle.
pub(crate) struct SuggestDb {
    pub conn: Mutex<Connection>,

    /// An object that's used to interrupt an ongoing database operation.
    ///
    /// When this handle is interrupted, the thread that's currently accessing
    /// the database will be told to stop and release the `conn` lock as soon
    /// as possible.
    pub interrupt_handle: Arc<SqlInterruptHandle>,
}

impl SuggestDb {
    /// Opens a read-only or read-write connection to a Suggest database at the
    /// given path.
    pub fn open(path: impl AsRef<Path>, type_: ConnectionType) -> Result<Self> {
        let conn = open_database_with_flags(path, type_.into(), &SuggestConnectionInitializer)?;
        Ok(Self::with_connection(conn))
    }

    fn with_connection(conn: Connection) -> Self {
        let interrupt_handle = Arc::new(SqlInterruptHandle::new(&conn));
        Self {
            conn: Mutex::new(conn),
            interrupt_handle,
        }
    }

    /// Accesses the Suggest database for reading.
    pub fn read<T>(&self, op: impl FnOnce(&SuggestDao) -> Result<T>) -> Result<T> {
        let conn = self.conn.lock();
        let scope = self.interrupt_handle.begin_interrupt_scope()?;
        let dao = SuggestDao::new(&conn, scope);
        op(&dao)
    }

    /// Accesses the Suggest database in a transaction for reading and writing.
    pub fn write<T>(&self, op: impl FnOnce(&mut SuggestDao) -> Result<T>) -> Result<T> {
        let mut conn = self.conn.lock();
        let scope = self.interrupt_handle.begin_interrupt_scope()?;
        let tx = conn.transaction()?;
        let mut dao = SuggestDao::new(&tx, scope);
        let result = op(&mut dao)?;
        tx.commit()?;
        Ok(result)
    }
}

/// A data access object (DAO) that wraps a connection to the Suggest database
/// with methods for reading and writing suggestions, icons, and metadata.
///
/// Methods that only read from the database take an immutable reference to
/// `self` (`&self`), and methods that write to the database take a mutable
/// reference (`&mut self`).
pub(crate) struct SuggestDao<'a> {
    pub conn: &'a Connection,
    pub scope: SqlInterruptScope,
}

impl<'a> SuggestDao<'a> {
    fn new(conn: &'a Connection, scope: SqlInterruptScope) -> Self {
        Self { conn, scope }
    }

    // =============== High level API ===============
    //
    //  These methods combine several low-level calls into one logical operation.

    pub fn handle_unparsable_record(&mut self, record: &RemoteSettingsRecord) -> Result<()> {
        let record_id = SuggestRecordId::from(&record.id);
        // Remember this record's ID so that we will try again later
        self.put_unparsable_record_id(&record_id)?;
        // Advance the last fetch time, so that we can resume
        // fetching after this record if we're interrupted.
        self.put_last_ingest_if_newer(record.last_modified)
    }

    pub fn handle_ingested_record(&mut self, record: &RemoteSettingsRecord) -> Result<()> {
        let record_id = SuggestRecordId::from(&record.id);
        // Remove this record's ID from the list of unparsable
        // records, since we understand it now.
        self.drop_unparsable_record_id(&record_id)?;
        // Advance the last fetch time, so that we can resume
        // fetching after this record if we're interrupted.
        self.put_last_ingest_if_newer(record.last_modified)
    }

    pub fn handle_deleted_record(&mut self, record: &RemoteSettingsRecord) -> Result<()> {
        let record_id = SuggestRecordId::from(&record.id);
        // Drop either the icon or suggestions, records only contain one or the other
        match record_id.as_icon_id() {
            Some(icon_id) => self.drop_icon(icon_id)?,
            None => self.drop_suggestions(&record_id)?,
        };
        // Remove this record's ID from the list of unparsable
        // records, since we understand it now.
        self.drop_unparsable_record_id(&record_id)?;
        // Advance the last fetch time, so that we can resume
        // fetching after this record if we're interrupted.
        self.put_last_ingest_if_newer(record.last_modified)
    }

    // =============== Low level API ===============
    //
    //  These methods implement CRUD operations

    /// Fetches suggestions that match the given query from the database.
    pub fn fetch_suggestions(&self, query: &SuggestionQuery) -> Result<Vec<Suggestion>> {
        let unique_providers = query.providers.iter().collect::<HashSet<_>>();
        unique_providers
            .iter()
            .try_fold(vec![], |mut acc, provider| {
                let suggestions = match provider {
                    SuggestionProvider::Amp => {
                        self.fetch_amp_suggestions(query, AmpSuggestionType::Desktop)
                    }
                    SuggestionProvider::AmpMobile => {
                        self.fetch_amp_suggestions(query, AmpSuggestionType::Mobile)
                    }
                    SuggestionProvider::Wikipedia => self.fetch_wikipedia_suggestions(query),
                    SuggestionProvider::Amo => self.fetch_amo_suggestions(query),
                    SuggestionProvider::Pocket => self.fetch_pocket_suggestions(query),
                    SuggestionProvider::Yelp => self.fetch_yelp_suggestions(query),
                    SuggestionProvider::Mdn => self.fetch_mdn_suggestions(query),
                    SuggestionProvider::Weather => self.fetch_weather_suggestions(query),
                }?;
                acc.extend(suggestions);
                Ok(acc)
            })
            .map(|mut suggestions| {
                suggestions.sort();
                if let Some(limit) = query.limit.and_then(|limit| usize::try_from(limit).ok()) {
                    suggestions.truncate(limit);
                }
                suggestions
            })
    }

    /// Fetches Suggestions of type Amp provider that match the given query
    pub fn fetch_amp_suggestions(
        &self,
        query: &SuggestionQuery,
        suggestion_type: AmpSuggestionType,
    ) -> Result<Vec<Suggestion>> {
        let keyword_lowercased = &query.keyword.to_lowercase();
        let provider = match suggestion_type {
            AmpSuggestionType::Mobile => SuggestionProvider::AmpMobile,
            AmpSuggestionType::Desktop => SuggestionProvider::Amp,
        };
        let suggestions = self.conn.query_rows_and_then_cached(
            r#"
            SELECT
              s.id,
              k.rank,
              s.title,
              s.url,
              s.provider,
              s.score
            FROM
              suggestions s
            JOIN
              keywords k
              ON k.suggestion_id = s.id
            WHERE
              s.provider = :provider
              AND k.keyword = :keyword
            "#,
            named_params! {
                ":keyword": keyword_lowercased,
                ":provider": provider
            },
            |row| -> Result<Suggestion> {
                let suggestion_id: i64 = row.get("id")?;
                let title = row.get("title")?;
                let raw_url = row.get::<_, String>("url")?;
                let score = row.get::<_, f64>("score")?;

                let keywords: Vec<String> = self.conn.query_rows_and_then_cached(
                    r#"
                    SELECT
                        keyword
                    FROM
                        keywords
                    WHERE
                        suggestion_id = :suggestion_id
                        AND rank >= :rank
                    ORDER BY
                        rank ASC
                    "#,
                    named_params! {
                        ":suggestion_id": suggestion_id,
                        ":rank": row.get::<_, i64>("rank")?,
                    },
                    |row| row.get(0),
                )?;
                self.conn.query_row_and_then(
                    r#"
                    SELECT
                      amp.advertiser,
                      amp.block_id,
                      amp.iab_category,
                      amp.impression_url,
                      amp.click_url,
                      (SELECT i.data FROM icons i WHERE i.id = amp.icon_id) AS icon
                    FROM
                      amp_custom_details amp
                    WHERE
                      amp.suggestion_id = :suggestion_id
                    "#,
                    named_params! {
                        ":suggestion_id": suggestion_id
                    },
                    |row| {
                        let cooked_url = cook_raw_suggestion_url(&raw_url);
                        let raw_click_url = row.get::<_, String>("click_url")?;
                        let cooked_click_url = cook_raw_suggestion_url(&raw_click_url);

                        Ok(Suggestion::Amp {
                            block_id: row.get("block_id")?,
                            advertiser: row.get("advertiser")?,
                            iab_category: row.get("iab_category")?,
                            title,
                            url: cooked_url,
                            raw_url,
                            full_keyword: full_keyword(keyword_lowercased, &keywords),
                            icon: row.get("icon")?,
                            impression_url: row.get("impression_url")?,
                            click_url: cooked_click_url,
                            raw_click_url,
                            score,
                        })
                    },
                )
            },
        )?;
        Ok(suggestions)
    }

    /// Fetches Suggestions of type Wikipedia provider that match the given query
    pub fn fetch_wikipedia_suggestions(&self, query: &SuggestionQuery) -> Result<Vec<Suggestion>> {
        let keyword_lowercased = &query.keyword.to_lowercase();
        let suggestions = self.conn.query_rows_and_then_cached(
            r#"
            SELECT
              s.id,
              k.rank,
              s.title,
              s.url
            FROM
              suggestions s
            JOIN
              keywords k
              ON k.suggestion_id = s.id
            WHERE
              s.provider = :provider
              AND k.keyword = :keyword
            "#,
            named_params! {
                ":keyword": keyword_lowercased,
                ":provider": SuggestionProvider::Wikipedia
            },
            |row| -> Result<Suggestion> {
                let suggestion_id: i64 = row.get("id")?;
                let title = row.get("title")?;
                let raw_url = row.get::<_, String>("url")?;

                let keywords: Vec<String> = self.conn.query_rows_and_then_cached(
                    "SELECT keyword FROM keywords
                     WHERE suggestion_id = :suggestion_id AND rank >= :rank
                     ORDER BY rank ASC",
                    named_params! {
                        ":suggestion_id": suggestion_id,
                        ":rank": row.get::<_, i64>("rank")?,
                    },
                    |row| row.get(0),
                )?;
                let icon = self.conn.try_query_one(
                    "SELECT i.data
                     FROM icons i
                     JOIN wikipedia_custom_details s ON s.icon_id = i.id
                     WHERE s.suggestion_id = :suggestion_id",
                    named_params! {
                        ":suggestion_id": suggestion_id
                    },
                    true,
                )?;
                Ok(Suggestion::Wikipedia {
                    title,
                    url: raw_url,
                    full_keyword: full_keyword(keyword_lowercased, &keywords),
                    icon,
                })
            },
        )?;
        Ok(suggestions)
    }

    /// Fetches Suggestions of type Amo provider that match the given query
    pub fn fetch_amo_suggestions(&self, query: &SuggestionQuery) -> Result<Vec<Suggestion>> {
        let keyword_lowercased = &query.keyword.to_lowercase();
        let (keyword_prefix, keyword_suffix) = split_keyword(keyword_lowercased);
        let suggestions_limit = &query.limit.unwrap_or(-1);
        let suggestions = self
            .conn
            .query_rows_and_then_cached(
                r#"
                SELECT
                  s.id,
                  MAX(k.rank) AS rank,
                  s.title,
                  s.url,
                  s.provider,
                  s.score,
                  k.keyword_suffix
                FROM
                  suggestions s
                JOIN
                  prefix_keywords k
                  ON k.suggestion_id = s.id
                WHERE
                  k.keyword_prefix = :keyword_prefix
                  AND (k.keyword_suffix BETWEEN :keyword_suffix AND :keyword_suffix || x'FFFF')
                  AND s.provider = :provider
                GROUP BY
                  s.id
                ORDER BY
                  s.score DESC,
                  rank DESC
                LIMIT
                  :suggestions_limit
                "#,
                named_params! {
                    ":keyword_prefix": keyword_prefix,
                    ":keyword_suffix": keyword_suffix,
                    ":provider": SuggestionProvider::Amo,
                    ":suggestions_limit": suggestions_limit,
                },
                |row| -> Result<Option<Suggestion>> {
                    let suggestion_id: i64 = row.get("id")?;
                    let title = row.get("title")?;
                    let raw_url = row.get::<_, String>("url")?;
                    let score = row.get::<_, f64>("score")?;

                    let full_suffix = row.get::<_, String>("keyword_suffix")?;
                    full_suffix
                        .starts_with(keyword_suffix)
                        .then(|| {
                            self.conn.query_row_and_then(
                                r#"
                                SELECT
                                  amo.description,
                                  amo.guid,
                                  amo.rating,
                                  amo.icon_url,
                                  amo.number_of_ratings
                                FROM
                                  amo_custom_details amo
                                WHERE
                                  amo.suggestion_id = :suggestion_id
                                "#,
                                named_params! {
                                    ":suggestion_id": suggestion_id
                                },
                                |row| {
                                    Ok(Suggestion::Amo {
                                        title,
                                        url: raw_url,
                                        icon_url: row.get("icon_url")?,
                                        description: row.get("description")?,
                                        rating: row.get("rating")?,
                                        number_of_ratings: row.get("number_of_ratings")?,
                                        guid: row.get("guid")?,
                                        score,
                                    })
                                },
                            )
                        })
                        .transpose()
                },
            )?
            .into_iter()
            .flatten()
            .collect();
        Ok(suggestions)
    }

    /// Fetches Suggestions of type pocket provider that match the given query
    pub fn fetch_pocket_suggestions(&self, query: &SuggestionQuery) -> Result<Vec<Suggestion>> {
        let keyword_lowercased = &query.keyword.to_lowercase();
        let (keyword_prefix, keyword_suffix) = split_keyword(keyword_lowercased);
        let suggestions = self
            .conn
            .query_rows_and_then_cached(
                r#"
            SELECT
              s.id,
              MAX(k.rank) AS rank,
              s.title,
              s.url,
              s.provider,
              s.score,
              k.confidence,
              k.keyword_suffix
            FROM
              suggestions s
            JOIN
              prefix_keywords k
              ON k.suggestion_id = s.id
            WHERE
              k.keyword_prefix = :keyword_prefix
              AND (k.keyword_suffix BETWEEN :keyword_suffix AND :keyword_suffix || x'FFFF')
              AND s.provider = :provider
            GROUP BY
              s.id,
              k.confidence
            ORDER BY
              s.score DESC,
              rank DESC
            "#,
                named_params! {
                    ":keyword_prefix": keyword_prefix,
                    ":keyword_suffix": keyword_suffix,
                    ":provider": SuggestionProvider::Pocket,
                },
                |row| -> Result<Option<Suggestion>> {
                    let title = row.get("title")?;
                    let raw_url = row.get::<_, String>("url")?;
                    let score = row.get::<_, f64>("score")?;
                    let confidence = row.get("confidence")?;
                    let full_suffix = row.get::<_, String>("keyword_suffix")?;
                    let suffixes_match = match confidence {
                        KeywordConfidence::Low => full_suffix.starts_with(keyword_suffix),
                        KeywordConfidence::High => full_suffix == keyword_suffix,
                    };
                    if suffixes_match {
                        Ok(Some(Suggestion::Pocket {
                            title,
                            url: raw_url,
                            score,
                            is_top_pick: matches!(confidence, KeywordConfidence::High),
                        }))
                    } else {
                        Ok(None)
                    }
                },
            )?
            .into_iter()
            .flatten()
            .take(
                query
                    .limit
                    .and_then(|limit| usize::try_from(limit).ok())
                    .unwrap_or(usize::MAX),
            )
            .collect();
        Ok(suggestions)
    }

    /// Fetches suggestions for MDN
    pub fn fetch_mdn_suggestions(&self, query: &SuggestionQuery) -> Result<Vec<Suggestion>> {
        let keyword_lowercased = &query.keyword.to_lowercase();
        let (keyword_prefix, keyword_suffix) = split_keyword(keyword_lowercased);
        let suggestions_limit = &query.limit.unwrap_or(-1);
        let suggestions = self
            .conn
            .query_rows_and_then_cached(
                r#"
                SELECT
                  s.id,
                  MAX(k.rank) AS rank,
                  s.title,
                  s.url,
                  s.provider,
                  s.score,
                  k.keyword_suffix
                FROM
                  suggestions s
                JOIN
                  prefix_keywords k
                  ON k.suggestion_id = s.id
                WHERE
                  k.keyword_prefix = :keyword_prefix
                  AND (k.keyword_suffix BETWEEN :keyword_suffix AND :keyword_suffix || x'FFFF')
                  AND s.provider = :provider
                GROUP BY
                  s.id
                ORDER BY
                  s.score DESC,
                  rank DESC
                LIMIT
                  :suggestions_limit
                "#,
                named_params! {
                    ":keyword_prefix": keyword_prefix,
                    ":keyword_suffix": keyword_suffix,
                    ":provider": SuggestionProvider::Mdn,
                    ":suggestions_limit": suggestions_limit,
                },
                |row| -> Result<Option<Suggestion>> {
                    let suggestion_id: i64 = row.get("id")?;
                    let title = row.get("title")?;
                    let raw_url = row.get::<_, String>("url")?;
                    let score = row.get::<_, f64>("score")?;

                    let full_suffix = row.get::<_, String>("keyword_suffix")?;
                    full_suffix
                        .starts_with(keyword_suffix)
                        .then(|| {
                            self.conn.query_row_and_then(
                                r#"
                                SELECT
                                    description
                                FROM
                                    mdn_custom_details
                                WHERE
                                    suggestion_id = :suggestion_id
                                "#,
                                named_params! {
                                    ":suggestion_id": suggestion_id
                                },
                                |row| {
                                    Ok(Suggestion::Mdn {
                                        title,
                                        url: raw_url,
                                        description: row.get("description")?,
                                        score,
                                    })
                                },
                            )
                        })
                        .transpose()
                },
            )?
            .into_iter()
            .flatten()
            .collect();

        Ok(suggestions)
    }

    /// Fetches weather suggestions
    pub fn fetch_weather_suggestions(&self, query: &SuggestionQuery) -> Result<Vec<Suggestion>> {
        // Weather keywords are matched by prefix but the query must be at least
        // three chars long. Unlike the prefix matching of other suggestion
        // types, the query doesn't need to contain the first full word.
        if query.keyword.len() < 3 {
            return Ok(vec![]);
        }

        let keyword_lowercased = &query.keyword.trim().to_lowercase();
        let suggestions = self.conn.query_rows_and_then_cached(
            r#"
            SELECT
              s.score
            FROM
              suggestions s
            JOIN
              keywords k
              ON k.suggestion_id = s.id
            WHERE
              s.provider = :provider
              AND (k.keyword BETWEEN :keyword AND :keyword || X'FFFF')
             "#,
            named_params! {
                ":keyword": keyword_lowercased,
                ":provider": SuggestionProvider::Weather
            },
            |row| -> Result<Suggestion> {
                Ok(Suggestion::Weather {
                    score: row.get::<_, f64>("score")?,
                })
            },
        )?;
        Ok(suggestions)
    }

    /// Inserts all suggestions from a downloaded AMO attachment into
    /// the database.
    pub fn insert_amo_suggestions(
        &mut self,
        record_id: &SuggestRecordId,
        suggestions: &[DownloadedAmoSuggestion],
    ) -> Result<()> {
        for suggestion in suggestions {
            self.scope.err_if_interrupted()?;
            let suggestion_id: i64 = self.conn.query_row_and_then_cachable(
                &format!(
                    "INSERT INTO suggestions(
                         record_id,
                         provider,
                         title,
                         url,
                         score
                     )
                     VALUES(
                         :record_id,
                         {},
                         :title,
                         :url,
                         :score
                     )
                     RETURNING id",
                    SuggestionProvider::Amo as u8
                ),
                named_params! {
                    ":record_id": record_id.as_str(),
                    ":title": suggestion.title,
                    ":url": suggestion.url,
                    ":score": suggestion.score,
                },
                |row| row.get(0),
                true,
            )?;
            self.conn.execute(
                "INSERT INTO amo_custom_details(
                             suggestion_id,
                             description,
                             guid,
                             icon_url,
                             rating,
                             number_of_ratings
                         )
                         VALUES(
                             :suggestion_id,
                             :description,
                             :guid,
                             :icon_url,
                             :rating,
                             :number_of_ratings
                         )",
                named_params! {
                    ":suggestion_id": suggestion_id,
                    ":description": suggestion.description,
                    ":guid": suggestion.guid,
                    ":icon_url": suggestion.icon_url,
                    ":rating": suggestion.rating,
                    ":number_of_ratings": suggestion.number_of_ratings
                },
            )?;
            for (index, keyword) in suggestion.keywords.iter().enumerate() {
                let (keyword_prefix, keyword_suffix) = split_keyword(keyword);
                self.conn.execute(
                    "INSERT INTO prefix_keywords(
                         keyword_prefix,
                         keyword_suffix,
                         suggestion_id,
                         rank
                     )
                     VALUES(
                         :keyword_prefix,
                         :keyword_suffix,
                         :suggestion_id,
                         :rank
                     )",
                    named_params! {
                        ":keyword_prefix": keyword_prefix,
                        ":keyword_suffix": keyword_suffix,
                        ":rank": index,
                        ":suggestion_id": suggestion_id,
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Inserts all suggestions from a downloaded AMP-Wikipedia attachment into
    /// the database.
    pub fn insert_amp_wikipedia_suggestions(
        &mut self,
        record_id: &SuggestRecordId,
        suggestions: &[DownloadedAmpWikipediaSuggestion],
    ) -> Result<()> {
        for suggestion in suggestions {
            self.scope.err_if_interrupted()?;
            let common_details = suggestion.common_details();
            let provider = suggestion.provider();

            let suggestion_id: i64 = self.conn.query_row_and_then_cachable(
                &format!(
                    "INSERT INTO suggestions(
                         record_id,
                         provider,
                         title,
                         url,
                         score
                     )
                     VALUES(
                         :record_id,
                         {},
                         :title,
                         :url,
                         :score
                     )
                     RETURNING id",
                    provider as u8
                ),
                named_params! {
                    ":record_id": record_id.as_str(),
                    ":title": common_details.title,
                    ":url": common_details.url,
                    ":score": common_details.score.unwrap_or(DEFAULT_SUGGESTION_SCORE)
                },
                |row| row.get(0),
                true,
            )?;
            match suggestion {
                DownloadedAmpWikipediaSuggestion::Amp(amp) => {
                    self.conn.execute(
                        "INSERT INTO amp_custom_details(
                             suggestion_id,
                             advertiser,
                             block_id,
                             iab_category,
                             impression_url,
                             click_url,
                             icon_id
                         )
                         VALUES(
                             :suggestion_id,
                             :advertiser,
                             :block_id,
                             :iab_category,
                             :impression_url,
                             :click_url,
                             :icon_id
                         )",
                        named_params! {
                            ":suggestion_id": suggestion_id,
                            ":advertiser": amp.advertiser,
                            ":block_id": amp.block_id,
                            ":iab_category": amp.iab_category,
                            ":impression_url": amp.impression_url,
                            ":click_url": amp.click_url,
                            ":icon_id": amp.icon_id,
                        },
                    )?;
                }
                DownloadedAmpWikipediaSuggestion::Wikipedia(wikipedia) => {
                    self.conn.execute(
                        "INSERT INTO wikipedia_custom_details(
                             suggestion_id,
                             icon_id
                         )
                         VALUES(
                             :suggestion_id,
                             :icon_id
                         )",
                        named_params! {
                            ":suggestion_id": suggestion_id,
                            ":icon_id": wikipedia.icon_id,
                        },
                    )?;
                }
            }
            for (index, keyword) in common_details.keywords.iter().enumerate() {
                self.conn.execute(
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
        Ok(())
    }

    /// Inserts all suggestions from a downloaded AMP-Mobile attachment into
    /// the database.
    pub fn insert_amp_mobile_suggestions(
        &mut self,
        record_id: &SuggestRecordId,
        suggestions: &[DownloadedAmpSuggestion],
    ) -> Result<()> {
        for suggestion in suggestions {
            self.scope.err_if_interrupted()?;
            let common_details = &suggestion.common_details;
            let suggestion_id: i64 = self.conn.query_row_and_then_cachable(
                &format!(
                    "INSERT INTO suggestions(
                         record_id,
                         provider,
                         title,
                         url,
                         score
                     )
                     VALUES(
                         :record_id,
                         {},
                         :title,
                         :url,
                         :score
                     )
                     RETURNING id",
                    SuggestionProvider::AmpMobile as u8
                ),
                named_params! {
                    ":record_id": record_id.as_str(),
                    ":title": common_details.title,
                    ":url": common_details.url,
                    ":score": common_details.score.unwrap_or(DEFAULT_SUGGESTION_SCORE)
                },
                |row| row.get(0),
                true,
            )?;
            self.conn.execute(
                "INSERT INTO amp_custom_details(
                     suggestion_id,
                     advertiser,
                     block_id,
                     iab_category,
                     impression_url,
                     click_url,
                     icon_id
                 )
                 VALUES(
                     :suggestion_id,
                     :advertiser,
                     :block_id,
                     :iab_category,
                     :impression_url,
                     :click_url,
                     :icon_id
                 )",
                named_params! {
                    ":suggestion_id": suggestion_id,
                    ":advertiser": suggestion.advertiser,
                    ":block_id": suggestion.block_id,
                    ":iab_category": suggestion.iab_category,
                    ":impression_url": suggestion.impression_url,
                    ":click_url": suggestion.click_url,
                    ":icon_id": suggestion.icon_id,
                },
            )?;

            for (index, keyword) in common_details.keywords.iter().enumerate() {
                self.conn.execute(
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
        Ok(())
    }

    /// Inserts all suggestions from a downloaded Pocket attachment into
    /// the database.
    pub fn insert_pocket_suggestions(
        &mut self,
        record_id: &SuggestRecordId,
        suggestions: &[DownloadedPocketSuggestion],
    ) -> Result<()> {
        for suggestion in suggestions {
            self.scope.err_if_interrupted()?;
            let suggestion_id: i64 = self.conn.query_row_and_then_cachable(
                &format!(
                    "INSERT INTO suggestions(
                         record_id,
                         provider,
                         title,
                         url,
                         score
                     )
                     VALUES(
                         :record_id,
                         {},
                         :title,
                         :url,
                         :score
                     )
                     RETURNING id",
                    SuggestionProvider::Pocket as u8
                ),
                named_params! {
                    ":record_id": record_id.as_str(),
                    ":title": suggestion.title,
                    ":url": suggestion.url,
                    ":score": suggestion.score,
                },
                |row| row.get(0),
                true,
            )?;

            for ((rank, keyword), confidence) in suggestion
                .high_confidence_keywords
                .iter()
                .enumerate()
                .zip(std::iter::repeat(KeywordConfidence::High))
                .chain(
                    suggestion
                        .low_confidence_keywords
                        .iter()
                        .enumerate()
                        .zip(std::iter::repeat(KeywordConfidence::Low)),
                )
            {
                let (keyword_prefix, keyword_suffix) = split_keyword(keyword);
                self.conn.execute(
                    "INSERT INTO prefix_keywords(
                             keyword_prefix,
                             keyword_suffix,
                             confidence,
                             rank,
                             suggestion_id
                         )
                         VALUES(
                             :keyword_prefix,
                             :keyword_suffix,
                             :confidence,
                             :rank,
                             :suggestion_id
                         )",
                    named_params! {
                        ":keyword_prefix": keyword_prefix,
                        ":keyword_suffix": keyword_suffix,
                        ":confidence": confidence,
                        ":rank": rank,
                        ":suggestion_id": suggestion_id,
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Inserts all suggestions from a downloaded MDN attachment into
    /// the database.
    pub fn insert_mdn_suggestions(
        &mut self,
        record_id: &SuggestRecordId,
        suggestions: &[DownloadedMdnSuggestion],
    ) -> Result<()> {
        for suggestion in suggestions {
            self.scope.err_if_interrupted()?;
            let suggestion_id: i64 = self.conn.query_row_and_then_cachable(
                &format!(
                    "INSERT INTO suggestions(
                         record_id,
                         provider,
                         title,
                         url,
                         score
                     )
                     VALUES(
                         :record_id,
                         {},
                         :title,
                         :url,
                         :score
                     )
                     RETURNING id",
                    SuggestionProvider::Mdn as u8
                ),
                named_params! {
                    ":record_id": record_id.as_str(),
                    ":title": suggestion.title,
                    ":url": suggestion.url,
                    ":score": suggestion.score,
                },
                |row| row.get(0),
                true,
            )?;
            self.conn.execute_cached(
                "INSERT INTO mdn_custom_details(
                     suggestion_id,
                     description
                 )
                 VALUES(
                     :suggestion_id,
                     :description
                 )",
                named_params! {
                    ":suggestion_id": suggestion_id,
                    ":description": suggestion.description,
                },
            )?;
            for (index, keyword) in suggestion.keywords.iter().enumerate() {
                let (keyword_prefix, keyword_suffix) = split_keyword(keyword);
                self.conn.execute_cached(
                    "INSERT INTO prefix_keywords(
                         keyword_prefix,
                         keyword_suffix,
                         suggestion_id,
                         rank
                     )
                     VALUES(
                         :keyword_prefix,
                         :keyword_suffix,
                         :suggestion_id,
                         :rank
                     )",
                    named_params! {
                        ":keyword_prefix": keyword_prefix,
                        ":keyword_suffix": keyword_suffix,
                        ":rank": index,
                        ":suggestion_id": suggestion_id,
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Inserts weather record data into the database.
    pub fn insert_weather_data(
        &mut self,
        record_id: &SuggestRecordId,
        data: &DownloadedWeatherData,
    ) -> Result<()> {
        self.scope.err_if_interrupted()?;
        let suggestion_id: i64 = self.conn.query_row_and_then_cachable(
            &format!(
                "INSERT INTO suggestions(record_id, provider, title, url, score)
                 VALUES(:record_id, {}, '', '', :score)
                 RETURNING id",
                SuggestionProvider::Weather as u8
            ),
            named_params! {
                ":record_id": record_id.as_str(),
                ":score": data.weather.score.unwrap_or(DEFAULT_SUGGESTION_SCORE),
            },
            |row| row.get(0),
            true,
        )?;
        for (index, keyword) in data.weather.keywords.iter().enumerate() {
            self.conn.execute(
                "INSERT INTO keywords(keyword, suggestion_id, rank)
                 VALUES(:keyword, :suggestion_id, :rank)",
                named_params! {
                    ":keyword": keyword,
                    ":suggestion_id": suggestion_id,
                    ":rank": index,
                },
            )?;
        }
        self.put_provider_config(
            SuggestionProvider::Weather,
            &SuggestProviderConfig::from(data),
        )?;
        Ok(())
    }

    /// Inserts or replaces an icon for a suggestion into the database.
    pub fn put_icon(&mut self, icon_id: &str, data: &[u8]) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO icons(
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

    /// Deletes all suggestions associated with a Remote Settings record from
    /// the database.
    pub fn drop_suggestions(&mut self, record_id: &SuggestRecordId) -> Result<()> {
        self.conn.execute_cached(
            "DELETE FROM suggestions WHERE record_id = :record_id",
            named_params! { ":record_id": record_id.as_str() },
        )?;
        self.conn.execute_cached(
            "DELETE FROM yelp_subjects WHERE record_id = :record_id",
            named_params! { ":record_id": record_id.as_str() },
        )?;
        self.conn.execute_cached(
            "DELETE FROM yelp_modifiers WHERE record_id = :record_id",
            named_params! { ":record_id": record_id.as_str() },
        )?;
        self.conn.execute_cached(
            "DELETE FROM yelp_location_signs WHERE record_id = :record_id",
            named_params! { ":record_id": record_id.as_str() },
        )?;
        self.conn.execute_cached(
            "DELETE FROM yelp_custom_details WHERE record_id = :record_id",
            named_params! { ":record_id": record_id.as_str() },
        )?;
        Ok(())
    }

    /// Deletes an icon for a suggestion from the database.
    pub fn drop_icon(&mut self, icon_id: &str) -> Result<()> {
        self.conn.execute_cached(
            "DELETE FROM icons WHERE id = :id",
            named_params! { ":id": icon_id },
        )?;
        Ok(())
    }

    /// Clears the database, removing all suggestions, icons, and metadata.
    pub fn clear(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "DELETE FROM suggestions;
             DELETE FROM icons;
             DELETE FROM meta;",
        )?;
        Ok(())
    }

    /// Returns the value associated with a metadata key.
    pub fn get_meta<T: FromSql>(&self, key: &str) -> Result<Option<T>> {
        Ok(self.conn.try_query_one(
            "SELECT value FROM meta WHERE key = :key",
            named_params! { ":key": key },
            true,
        )?)
    }

    /// Sets the value for a metadata key.
    pub fn put_meta(&mut self, key: &str, value: impl ToSql) -> Result<()> {
        self.conn.execute_cached(
            "INSERT OR REPLACE INTO meta(key, value) VALUES(:key, :value)",
            named_params! { ":key": key, ":value": value },
        )?;
        Ok(())
    }

    /// Updates the last ingest timestamp if the given last modified time is
    /// newer than the existing one recorded.
    pub fn put_last_ingest_if_newer(&mut self, record_last_modified: u64) -> Result<()> {
        let last_ingest = self
            .get_meta::<u64>(LAST_INGEST_META_KEY)?
            .unwrap_or_default();
        if record_last_modified > last_ingest {
            self.put_meta(LAST_INGEST_META_KEY, record_last_modified)?;
        }

        Ok(())
    }

    /// Adds an entry for a Suggest Remote Settings record to the list of
    /// unparsable records.
    ///
    /// This is used to note records that we don't understand how to parse and
    /// ingest yet.
    pub fn put_unparsable_record_id(&mut self, record_id: &SuggestRecordId) -> Result<()> {
        let mut unparsable_records = self
            .get_meta::<UnparsableRecords>(UNPARSABLE_RECORDS_META_KEY)?
            .unwrap_or_default();
        unparsable_records.0.insert(
            record_id.as_str().to_string(),
            UnparsableRecord {
                schema_version: VERSION,
            },
        );
        self.put_meta(UNPARSABLE_RECORDS_META_KEY, unparsable_records)?;
        Ok(())
    }

    /// Removes an entry for a Suggest Remote Settings record from the list of
    /// unparsable records. Does nothing if the record was not previously marked
    /// as unparsable.
    ///
    /// This indicates that we now understand how to parse and ingest the
    /// record, or that the record was deleted.
    pub fn drop_unparsable_record_id(&mut self, record_id: &SuggestRecordId) -> Result<()> {
        let Some(mut unparsable_records) =
            self.get_meta::<UnparsableRecords>(UNPARSABLE_RECORDS_META_KEY)?
        else {
            return Ok(());
        };
        if unparsable_records.0.remove(record_id.as_str()).is_none() {
            return Ok(());
        };
        self.put_meta(UNPARSABLE_RECORDS_META_KEY, unparsable_records)
    }

    /// Stores global Suggest configuration data.
    pub fn put_global_config(&mut self, config: &SuggestGlobalConfig) -> Result<()> {
        self.put_meta(GLOBAL_CONFIG_META_KEY, serde_json::to_string(config)?)
    }

    /// Gets the stored global Suggest configuration data or a default config if
    /// none is stored.
    pub fn get_global_config(&self) -> Result<SuggestGlobalConfig> {
        self.get_meta::<String>(GLOBAL_CONFIG_META_KEY)?
            .map_or_else(
                || Ok(SuggestGlobalConfig::default()),
                |json| Ok(serde_json::from_str(&json)?),
            )
    }

    /// Stores configuration data for a given provider.
    pub fn put_provider_config(
        &mut self,
        provider: SuggestionProvider,
        config: &SuggestProviderConfig,
    ) -> Result<()> {
        self.put_meta(
            &provider_config_meta_key(provider),
            serde_json::to_string(config)?,
        )
    }

    /// Gets the stored configuration data for a given provider or None if none
    /// is stored.
    pub fn get_provider_config(
        &self,
        provider: SuggestionProvider,
    ) -> Result<Option<SuggestProviderConfig>> {
        self.get_meta::<String>(&provider_config_meta_key(provider))?
            .map_or_else(|| Ok(None), |json| Ok(serde_json::from_str(&json)?))
    }
}

fn provider_config_meta_key(provider: SuggestionProvider) -> String {
    format!("{}{}", PROVIDER_CONFIG_META_KEY_PREFIX, provider as u8)
}

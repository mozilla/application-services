/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use rusqlite::named_params;
use serde::Deserialize;
use sql_support::ConnExt;

use crate::{
    config::SuggestProviderConfig,
    db::{
        KeywordInsertStatement, KeywordMetricsInsertStatement, SuggestDao,
        SuggestionInsertStatement, DEFAULT_SUGGESTION_SCORE,
    },
    geoname::{Geoname, GeonameType},
    metrics::DownloadTimer,
    provider::SuggestionProvider,
    rs::{Client, Record, SuggestRecordId},
    store::SuggestStoreInner,
    suggestion::Suggestion,
    util::filter_map_chunks,
    Result, SuggestionQuery,
};

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct DownloadedWeatherAttachment {
    /// Weather keywords.
    pub keywords: Vec<String>,
    /// Threshold for weather keyword prefix matching when a weather keyword is
    /// the first term in a query. `None` means prefix matching is disabled and
    /// weather keywords must be typed in full when they are first in the query.
    /// This threshold does not apply to city and region names. If there are
    /// multiple weather records, we use the `min_keyword_length` in the most
    /// recently ingested record.
    pub min_keyword_length: Option<i32>,
    /// Score for weather suggestions. If there are multiple weather records, we
    /// use the `score` from the most recently ingested record.
    pub score: Option<f64>,
    /// The max length of all keywords in the attachment. Used for keyword
    /// metrics. We pre-compute this to avoid doing duplicate work on all user's
    /// machines.
    pub max_keyword_length: u32,
    /// The max word count of all keywords in the attachment. Used for keyword
    /// metrics. We pre-compute this to avoid doing duplicate work on all user's
    /// machines.
    pub max_keyword_word_count: u32,
}

/// This data is used to service every query handled by the weather provider, so
/// we cache it from the DB.
#[derive(Debug, Default)]
pub struct WeatherCache {
    /// Cached value of the same name from `SuggestProviderConfig::Weather`.
    min_keyword_length: i32,
    /// Cached value of the same name from `SuggestProviderConfig::Weather`.
    score: f64,
    /// Max length of all weather keywords.
    max_keyword_length: usize,
    /// Max word count across all weather keywords.
    max_keyword_word_count: usize,
}

impl SuggestDao<'_> {
    /// Fetches weather suggestions.
    pub fn fetch_weather_suggestions(&self, query: &SuggestionQuery) -> Result<Vec<Suggestion>> {
        // We'll just stipulate we won't support tiny queries in order to avoid
        // a bunch of work when the user starts typing a query.
        if query.keyword.len() < 3 {
            return Ok(vec![]);
        }

        // The first step in parsing the query is lowercasing and splitting it
        // into words. We want to avoid that work for strings that are so long
        // they can't possibly match. The longest possible weather query is two
        // geonames + one weather keyword + at least two spaces between those
        // three components, say, 10 spaces total for some wiggle room. There's
        // not much point in an analogous min length check since weather
        // suggestions can be matched on city alone and many city names are only
        // a few characters long ("nyc").
        let g_cache = self.geoname_cache();
        let w_cache = self.weather_cache();
        let max_query_len = 2 * g_cache.max_name_length + w_cache.max_keyword_length + 10;
        if max_query_len < query.keyword.len() {
            return Ok(vec![]);
        }

        let max_win_size =
            std::cmp::max(g_cache.max_name_word_count, w_cache.max_keyword_word_count);
        let kw = query.keyword.to_lowercase();
        let paths = filter_map_chunks::<Token>(&kw, max_win_size, |chunk, chunk_index, path| {
            // Match the chunk to a token type that hasn't already been matched
            // in this path.
            for tt in [
                TokenType::City,
                TokenType::Region,
                TokenType::WeatherKeyword,
            ] {
                if !path.iter().any(|t| t.token_type() == tt) {
                    let tokens = self.match_weather_tokens(tt, path, chunk, chunk_index == 0)?;
                    if !tokens.is_empty() {
                        // This chunk matches, so return the tokens to continue
                        // down this path.
                        return Ok(Some(tokens));
                    }
                }
            }
            // This chunk doesn't match. Return `None` to filter out the path.
            Ok(None)
        })?;

        // `paths` contains all matching paths, and if a path has both a city
        // and region, the city is in the region since we filtered out city-
        // region mismatches above via `match_weather_tokens()`.
        Ok(paths
            .into_iter()
            .filter_map(|path| {
                let weather_kw = path.iter().find_map(|t| t.weather_keyword());
                let city = path.iter().find_map(|t| t.city());
                let region = path.iter().find_map(|t| t.region());
                match (weather_kw, city, region) {
                    (None, None, _) | (Some(_), None, Some(_)) => None,
                    (Some(_), None, None) => Some(Suggestion::Weather {
                        city: None,
                        region: None,
                        score: w_cache.score,
                    }),
                    (Some(_), Some(c), None)
                    | (Some(_), Some(c), Some(_))
                    | (None, Some(c), None)
                    | (None, Some(c), Some(_)) => Some(Suggestion::Weather {
                        city: Some(c.name.clone()),
                        region: Some(c.admin1_code.clone()),
                        score: w_cache.score,
                    }),
                }
            })
            .collect())
    }

    fn match_weather_tokens(
        &self,
        token_type: TokenType,
        path: &[Token],
        candidate: &str,
        is_first_chunk: bool,
    ) -> Result<Vec<Token>> {
        match token_type {
            TokenType::City => {
                // Fetch matching cities, and filter them to regions we've
                // already matched in this path. Allow prefix matching for
                // chunks after the first.
                let regions: Vec<_> = path.iter().filter_map(|t| t.region()).collect();
                Ok(self
                    .fetch_geonames(
                        candidate,
                        !is_first_chunk,
                        Some(GeonameType::City),
                        if regions.is_empty() {
                            None
                        } else {
                            Some(regions)
                        },
                    )?
                    .into_iter()
                    .map(Token::City)
                    .collect())
            }
            TokenType::Region => {
                // Fetch matching regions, and filter them to cities we've
                // already matched in this patch. Allow prefix matching for
                // chunks after the first.
                let cities: Vec<_> = path.iter().filter_map(|t| t.city()).collect();
                Ok(self
                    .fetch_geonames(
                        candidate,
                        !is_first_chunk,
                        Some(GeonameType::Region),
                        if cities.is_empty() {
                            None
                        } else {
                            Some(cities)
                        },
                    )?
                    .into_iter()
                    .map(Token::Region)
                    .collect())
            }
            TokenType::WeatherKeyword => {
                // Fetch matching keywords.
                let len = self.weather_cache().min_keyword_length;
                if is_first_chunk && (candidate.len() as i32) < len {
                    // The candidate is the first chunk and it's too short.
                    Ok(vec![])
                } else {
                    // Allow arbitrary prefix matching if the candidate isn't
                    // the first chunk or if prefix matching is allowed.
                    Ok(self
                        .match_weather_keywords(candidate, !is_first_chunk || len > 0)?
                        .into_iter()
                        .map(Token::WeatherKeyword)
                        .collect())
                }
            }
        }
    }

    fn match_weather_keywords(&self, candidate: &str, prefix: bool) -> Result<Vec<String>> {
        self.conn.query_rows_and_then_cached(
            r#"
            SELECT
                k.keyword,
                s.score
            FROM
                suggestions s
            JOIN
                keywords k
                ON k.suggestion_id = s.id
            WHERE
                s.provider = :provider
                AND (
                    CASE :prefix WHEN FALSE THEN k.keyword = :keyword
                    ELSE (k.keyword BETWEEN :keyword AND :keyword || X'FFFF') END
                )
             "#,
            named_params! {
                ":prefix": prefix,
                ":keyword": candidate,
                ":provider": SuggestionProvider::Weather
            },
            |row| -> Result<String> { Ok(row.get("keyword")?) },
        )
    }

    /// Inserts weather suggestions data into the database.
    fn insert_weather_data(
        &mut self,
        record_id: &SuggestRecordId,
        attachments: &[DownloadedWeatherAttachment],
    ) -> Result<()> {
        self.scope.err_if_interrupted()?;
        let mut suggestion_insert = SuggestionInsertStatement::new(self.conn)?;
        let mut keyword_insert = KeywordInsertStatement::new(self.conn)?;
        let mut keyword_metrics_insert = KeywordMetricsInsertStatement::new(self.conn)?;
        let mut max_len = 0;
        let mut max_word_count = 0;
        for attach in attachments {
            let suggestion_id = suggestion_insert.execute(
                record_id,
                "",
                "",
                attach.score.unwrap_or(DEFAULT_SUGGESTION_SCORE),
                SuggestionProvider::Weather,
            )?;
            for (i, keyword) in attach.keywords.iter().enumerate() {
                keyword_insert.execute(suggestion_id, keyword, None, i)?;
            }
            self.put_provider_config(SuggestionProvider::Weather, &attach.into())?;
            max_len = std::cmp::max(max_len, attach.max_keyword_length as usize);
            max_word_count = std::cmp::max(max_word_count, attach.max_keyword_word_count as usize);
        }

        // Update keyword metrics.
        keyword_metrics_insert.execute(
            record_id,
            SuggestionProvider::Weather,
            max_len,
            max_word_count,
        )?;

        // We just made some insertions that might invalidate the data in the
        // cache. Clear it so it's repopulated the next time it's accessed.
        self.weather_cache.take();

        Ok(())
    }

    fn weather_cache(&self) -> &WeatherCache {
        self.weather_cache.get_or_init(|| {
            let mut cache = WeatherCache::default();

            // keyword metrics
            if let Ok((len, word_count)) = self.conn.query_row_and_then(
                r#"
                SELECT
                    max(max_length) AS len, max(max_word_count) AS word_count
                FROM
                    keywords_metrics
                WHERE
                    provider = :provider
                "#,
                named_params! {
                    ":provider": SuggestionProvider::Weather
                },
                |row| -> Result<(usize, usize)> { Ok((row.get("len")?, row.get("word_count")?)) },
            ) {
                cache.max_keyword_length = len;
                cache.max_keyword_word_count = word_count;
            }

            // provider config
            if let Ok(Some(SuggestProviderConfig::Weather {
                score,
                min_keyword_length,
            })) = self.get_provider_config(SuggestionProvider::Weather)
            {
                cache.min_keyword_length = min_keyword_length;
                cache.score = score;
            }

            cache
        })
    }
}

impl<S> SuggestStoreInner<S>
where
    S: Client,
{
    /// Inserts a weather record into the database.
    pub fn process_weather_record(
        &self,
        dao: &mut SuggestDao,
        record: &Record,
        download_timer: &mut DownloadTimer,
    ) -> Result<()> {
        self.download_attachment(dao, record, download_timer, |dao, record_id, data| {
            dao.insert_weather_data(record_id, data)
        })
    }
}

impl From<&DownloadedWeatherAttachment> for SuggestProviderConfig {
    fn from(a: &DownloadedWeatherAttachment) -> Self {
        Self::Weather {
            score: a.score.unwrap_or(DEFAULT_SUGGESTION_SCORE),
            min_keyword_length: a.min_keyword_length.unwrap_or(0),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum TokenType {
    City,
    Region,
    WeatherKeyword,
}

#[derive(Clone, Debug)]
enum Token {
    City(Geoname),
    Region(Geoname),
    WeatherKeyword(String),
}

impl Token {
    fn city(&self) -> Option<&Geoname> {
        match self {
            Self::City(g) => Some(g),
            _ => None,
        }
    }

    fn region(&self) -> Option<&Geoname> {
        match self {
            Self::Region(g) => Some(g),
            _ => None,
        }
    }

    fn weather_keyword(&self) -> Option<&str> {
        match self {
            Self::WeatherKeyword(s) => Some(s),
            _ => None,
        }
    }

    fn token_type(&self) -> TokenType {
        match self {
            Self::City(_) => TokenType::City,
            Self::Region(_) => TokenType::Region,
            Self::WeatherKeyword(_) => TokenType::WeatherKeyword,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{store::tests::TestStore, testing::*, SuggestIngestionConstraints};

    #[test]
    fn weather_provider_config() -> anyhow::Result<()> {
        before_each();
        let store = TestStore::new(MockRemoteSettingsClient::default().with_record(
            "weather",
            "weather-1",
            json!({
                "min_keyword_length": 3,
                "keywords": ["ab", "xyz", "weather"],
                "max_keyword_length": "weather".len(),
                "max_keyword_word_count": 1,
                "score": 0.24
            }),
        ));
        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });
        assert_eq!(
            store.fetch_provider_config(SuggestionProvider::Weather),
            Some(SuggestProviderConfig::Weather {
                score: 0.24,
                min_keyword_length: 3,
            })
        );
        Ok(())
    }

    #[test]
    fn weather_keywords_prefixes_allowed() -> anyhow::Result<()> {
        before_each();

        let store = TestStore::new(MockRemoteSettingsClient::default().with_record(
            "weather",
            "weather-1",
            json!({
                // min_keyword_length > 0 means prefixes are allowed.
                "min_keyword_length": 3,
                "keywords": ["ab", "xyz", "weather"],
                "max_keyword_length": "weather".len(),
                "max_keyword_word_count": 1,
                "score": 0.24
            }),
        ));

        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });

        let no_matches = [
            // doesn't match any keyword
            "xab",
            "abx",
            "xxyz",
            "xyzx",
            "weatherx",
            "xweather",
            "xwea",
            "x   weather",
            "   weather x",
            "weather foo",
            "foo weather",
            // too short
            "xy",
            "ab",
            "we",
        ];
        for q in no_matches {
            assert_eq!(store.fetch_suggestions(SuggestionQuery::weather(q)), vec![]);
        }

        let matches = [
            "xyz",
            "wea",
            "weat",
            "weath",
            "weathe",
            "weather",
            "WeAtHeR",
            "  weather  ",
        ];
        for q in matches {
            assert_eq!(
                store.fetch_suggestions(SuggestionQuery::weather(q)),
                vec![Suggestion::Weather {
                    score: 0.24,
                    city: None,
                    region: None
                },]
            );
        }

        Ok(())
    }

    #[test]
    fn weather_keywords_prefixes_not_allowed() -> anyhow::Result<()> {
        before_each();

        let store = TestStore::new(MockRemoteSettingsClient::default().with_record(
            "weather",
            "weather-1",
            json!({
                // min_keyword_length == 0 means prefixes are not allowed.
                "min_keyword_length": 0,
                "keywords": ["weather"],
                "max_keyword_length": "weather".len(),
                "max_keyword_word_count": 1,
                "score": 0.24
            }),
        ));

        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });

        let no_matches = ["wea", "weat", "weath", "weathe"];
        for q in no_matches {
            assert_eq!(store.fetch_suggestions(SuggestionQuery::weather(q)), vec![]);
        }

        let matches = ["weather", "WeAtHeR", "  weather  "];
        for q in matches {
            assert_eq!(
                store.fetch_suggestions(SuggestionQuery::weather(q)),
                vec![Suggestion::Weather {
                    score: 0.24,
                    city: None,
                    region: None
                },]
            );
        }

        Ok(())
    }

    #[test]
    fn cities_and_regions() -> anyhow::Result<()> {
        before_each();

        let mut store = crate::geoname::tests::new_test_store();
        store.client_mut().add_record(
            "weather",
            "weather-1",
            json!({
                "keywords": ["ab", "xyz", "weather"],
                "min_keyword_length": 3,
                "max_keyword_length": "weather".len(),
                "max_keyword_word_count": 1,
                "score": 0.24
            }),
        );

        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });

        let tests: &[(&str, Vec<Suggestion>)] = &[
            (
                "WaTeRlOo",
                vec![
                    // Waterloo, IA should be first since its population is
                    // larger than Waterloo, AL.
                    Suggestion::Weather {
                        city: Some("Waterloo".to_string()),
                        region: Some("IA".to_string()),
                        score: 0.24,
                    },
                    Suggestion::Weather {
                        city: Some("Waterloo".to_string()),
                        region: Some("AL".to_string()),
                        score: 0.24,
                    },
                ],
            ),
            (
                "waterloo Ia",
                vec![Suggestion::Weather {
                    city: Some("Waterloo".to_string()),
                    region: Some("IA".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "ia waterloo",
                vec![Suggestion::Weather {
                    city: Some("Waterloo".to_string()),
                    region: Some("IA".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "waterloo Al",
                vec![Suggestion::Weather {
                    city: Some("Waterloo".to_string()),
                    region: Some("AL".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "al waterloo",
                vec![Suggestion::Weather {
                    city: Some("Waterloo".to_string()),
                    region: Some("AL".to_string()),
                    score: 0.24,
                }],
            ),
            ("waterloo ia al", vec![]),
            ("waterloo ny", vec![]),
            (
                "new york",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "new york new york",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "ny ny",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            ("ny ny ny", vec![]),
            (
                "ny new york",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "new york ny",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "weather ny",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "ny weather",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "weather ny ny",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "ny weather ny",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "ny ny weather",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "weather new york",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "new york weather",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "weather new york new york",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "new york weather new york",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "new york new york weather",
                vec![Suggestion::Weather {
                    city: Some("New York City".to_string()),
                    region: Some("NY".to_string()),
                    score: 0.24,
                }],
            ),
            (
                "weather water",
                vec![
                    Suggestion::Weather {
                        city: Some("Waterloo".to_string()),
                        region: Some("IA".to_string()),
                        score: 0.24,
                    },
                    Suggestion::Weather {
                        city: Some("Waterloo".to_string()),
                        region: Some("AL".to_string()),
                        score: 0.24,
                    },
                ],
            ),
            (
                "waterloo w",
                vec![
                    Suggestion::Weather {
                        city: Some("Waterloo".to_string()),
                        region: Some("IA".to_string()),
                        score: 0.24,
                    },
                    Suggestion::Weather {
                        city: Some("Waterloo".to_string()),
                        region: Some("AL".to_string()),
                        score: 0.24,
                    },
                ],
            ),
            ("waterloo foo", vec![]),
            ("waterloo weather foo", vec![]),
            ("foo waterloo", vec![]),
            ("foo waterloo weather", vec![]),
        ];

        for (query, expected_suggestions) in tests {
            assert_eq!(
                &store.fetch_suggestions(SuggestionQuery::weather(query)),
                expected_suggestions
            );
        }

        Ok(())
    }

    #[test]
    fn keywords_metrics() -> anyhow::Result<()> {
        before_each();

        // Add a couple of records with different metrics. We're just testing
        // metrics so the other values don't matter.
        let mut store = TestStore::new(
            MockRemoteSettingsClient::default()
                .with_record(
                    "weather",
                    "weather-0",
                    json!({
                        "max_keyword_length": 10,
                        "max_keyword_word_count": 5,
                        "min_keyword_length": 3,
                        "score": 0.24,
                        "keywords": []
                    }),
                )
                .with_record(
                    "weather",
                    "weather-1",
                    json!({
                        "max_keyword_length": 20,
                        "max_keyword_word_count": 2,
                        "min_keyword_length": 3,
                        "score": 0.24,
                        "keywords": []
                    }),
                ),
        );

        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });

        store.read(|dao| {
            let cache = dao.weather_cache();
            assert_eq!(cache.max_keyword_length, 20);
            assert_eq!(cache.max_keyword_word_count, 5);
            Ok(())
        })?;

        // Delete the first record. The metrics should change.
        store
            .client_mut()
            .delete_record("quicksuggest", "weather-0");
        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });
        store.read(|dao| {
            let cache = dao.weather_cache();
            assert_eq!(cache.max_keyword_length, 20);
            assert_eq!(cache.max_keyword_word_count, 2);
            Ok(())
        })?;

        // Add a new record. The metrics should change again.
        store.client_mut().add_record(
            "weather",
            "weather-3",
            json!({
                "max_keyword_length": 15,
                "max_keyword_word_count": 3,
                "min_keyword_length": 3,
                "score": 0.24,
                "keywords": []
            }),
        );
        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });
        store.read(|dao| {
            let cache = dao.weather_cache();
            assert_eq!(cache.max_keyword_length, 20);
            assert_eq!(cache.max_keyword_word_count, 3);
            Ok(())
        })?;

        Ok(())
    }
}

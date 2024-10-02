use rusqlite::named_params;
use serde::Deserialize;
use sql_support::ConnExt;

use crate::{
    config::SuggestProviderConfig,
    db::{KeywordInsertStatement, SuggestDao, SuggestionInsertStatement, DEFAULT_SUGGESTION_SCORE},
    metrics::DownloadTimer,
    provider::SuggestionProvider,
    rs::{Client, Record, SuggestRecordId},
    store::SuggestStoreInner,
    suggestion::Suggestion,
    Result, SuggestionQuery,
};

impl SuggestDao<'_> {
    /// Fetches weather suggestions.
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

    /// Inserts weather suggestions data into the database.
    fn insert_weather_data(
        &mut self,
        record_id: &SuggestRecordId,
        attachments: &[DownloadedWeatherAttachment],
    ) -> Result<()> {
        self.scope.err_if_interrupted()?;
        let mut suggestion_insert = SuggestionInsertStatement::new(self.conn)?;
        let mut keyword_insert = KeywordInsertStatement::new(self.conn)?;
        for attachment in attachments {
            let suggestion_id = suggestion_insert.execute(
                record_id,
                "",
                "",
                attachment.score.unwrap_or(DEFAULT_SUGGESTION_SCORE),
                SuggestionProvider::Weather,
            )?;
            for (i, keyword) in attachment.keywords.iter().enumerate() {
                keyword_insert.execute(suggestion_id, keyword, None, i)?;
            }
            self.put_provider_config(SuggestionProvider::Weather, &attachment.into())?;
        }
        Ok(())
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

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct DownloadedWeatherAttachment {
    pub keywords: Vec<String>,
    pub min_keyword_length: i32,
    pub score: Option<f64>,
}

impl From<&DownloadedWeatherAttachment> for SuggestProviderConfig {
    fn from(a: &DownloadedWeatherAttachment) -> Self {
        Self::Weather {
            min_keyword_length: a.min_keyword_length,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{store::tests::TestStore, testing::*, SuggestIngestionConstraints};

    #[test]
    fn weather() -> anyhow::Result<()> {
        before_each();

        let store = TestStore::new(MockRemoteSettingsClient::default().with_record(
            "weather",
            "weather-1",
            json!({
                "keywords": ["ab", "xyz", "weather"],
                "min_keyword_length": 3,
                "score": 0.24
            }),
        ));

        store.ingest(SuggestIngestionConstraints {
            providers: Some(vec![SuggestionProvider::Weather]),
            ..SuggestIngestionConstraints::all_providers()
        });

        // No match since the query doesn't match any keyword
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("xab")),
            vec![]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("abx")),
            vec![]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("xxyz")),
            vec![]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("xyzx")),
            vec![]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("weatherx")),
            vec![]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("xweather")),
            vec![]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("xwea")),
            vec![]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("x   weather")),
            vec![]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("   weather x")),
            vec![]
        );

        // No match since the query is too short
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("xy")),
            vec![]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("ab")),
            vec![]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("we")),
            vec![]
        );

        // Matches
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("xyz")),
            vec![Suggestion::Weather { score: 0.24 },]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("wea")),
            vec![Suggestion::Weather { score: 0.24 },]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("weat")),
            vec![Suggestion::Weather { score: 0.24 },]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("weath")),
            vec![Suggestion::Weather { score: 0.24 },]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("weathe")),
            vec![Suggestion::Weather { score: 0.24 },]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("weather")),
            vec![Suggestion::Weather { score: 0.24 },]
        );
        assert_eq!(
            store.fetch_suggestions(SuggestionQuery::weather("  weather  ")),
            vec![Suggestion::Weather { score: 0.24 },]
        );

        assert_eq!(
            store.fetch_provider_config(SuggestionProvider::Weather),
            Some(SuggestProviderConfig::Weather {
                min_keyword_length: 3,
            })
        );

        Ok(())
    }
}

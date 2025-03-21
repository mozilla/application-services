/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use rusqlite::types::ToSqlOutput;
use rusqlite::{named_params, Result as RusqliteResult, ToSql};
use sql_support::ConnExt;
use url::form_urlencoded;

use crate::{
    db::SuggestDao,
    provider::SuggestionProvider,
    rs::{DownloadedYelpSuggestion, SuggestRecordId},
    suggestion::Suggestion,
    Result, SuggestionQuery,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
enum Modifier {
    Pre = 0,
    Post = 1,
    Yelp = 2,
}

impl ToSql for Modifier {
    fn to_sql(&self) -> RusqliteResult<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(*self as u8))
    }
}

#[derive(Eq, PartialEq)]
enum FindFrom {
    First,
    Last,
}

/// This module assumes like following query.
/// "Yelp-modifier? Pre-modifier? Subject Post-modifier? (Location-modifier | Location-sign Location?)? Yelp-modifier?"
/// For example, the query below is valid.
/// "Yelp (Yelp-modifier) Best(Pre-modifier) Ramen(Subject) Delivery(Post-modifier) In(Location-sign) Tokyo(Location)"
/// Also, as everything except Subject is optional, "Ramen" will be also valid query.
/// However, "Best Best Ramen" and "Ramen Best" is out of the above appearance order rule,
/// parsing will be failed. Also, every words except Location needs to be registered in DB.
/// Please refer to the query test in store.rs for all of combination.
/// Currently, the maximum query length is determined while referring to having word lengths in DB
/// and location names.
/// max subject: 50 + pre-modifier: 10 + post-modifier: 10 + location-sign: 7 + location: 50 = 127 = 150.
const MAX_QUERY_LENGTH: usize = 150;

/// The max number of words consisting the modifier. To improve the SQL performance by matching with
/// "keyword=:modifier" (please see is_modifier()), define this how many words we should check.
const MAX_MODIFIER_WORDS_NUMBER: usize = 2;

/// The max number of words consisting the location sign. To improve the SQL performance by matching
/// with "keyword=:modifier" (please see is_location_sign()), define this how many words we should
/// check.
const MAX_LOCATOIN_SIGN_WORDS_NUMBER: usize = 2;

/// At least this many characters must be typed for a subject to be matched.
const SUBJECT_PREFIX_MATCH_THRESHOLD: usize = 2;

impl<'a> SuggestDao<'a> {
    /// Inserts the suggestions for Yelp attachment into the database.
    pub(crate) fn insert_yelp_suggestions(
        &mut self,
        record_id: &SuggestRecordId,
        suggestion: &DownloadedYelpSuggestion,
    ) -> Result<()> {
        for keyword in &suggestion.subjects {
            self.scope.err_if_interrupted()?;
            self.conn.execute_cached(
                "INSERT INTO yelp_subjects(record_id, keyword) VALUES(:record_id, :keyword)",
                named_params! {
                    ":record_id": record_id.as_str(),
                    ":keyword": keyword,
                },
            )?;
        }

        for keyword in &suggestion.pre_modifiers {
            self.scope.err_if_interrupted()?;
            self.conn.execute_cached(
                "INSERT INTO yelp_modifiers(record_id, type, keyword) VALUES(:record_id, :type, :keyword)",
                named_params! {
                    ":record_id": record_id.as_str(),
                    ":type": Modifier::Pre,
                    ":keyword": keyword,
                },
            )?;
        }

        for keyword in &suggestion.post_modifiers {
            self.scope.err_if_interrupted()?;
            self.conn.execute_cached(
                "INSERT INTO yelp_modifiers(record_id, type, keyword) VALUES(:record_id, :type, :keyword)",
                named_params! {
                    ":record_id": record_id.as_str(),
                    ":type": Modifier::Post,
                    ":keyword": keyword,
                },
            )?;
        }

        for keyword in &suggestion.yelp_modifiers {
            self.scope.err_if_interrupted()?;
            self.conn.execute_cached(
                "INSERT INTO yelp_modifiers(record_id, type, keyword) VALUES(:record_id, :type, :keyword)",
                named_params! {
                    ":record_id": record_id.as_str(),
                    ":type": Modifier::Yelp,
                    ":keyword": keyword,
                },
            )?;
        }

        for sign in &suggestion.location_signs {
            self.scope.err_if_interrupted()?;
            self.conn.execute_cached(
                "INSERT INTO yelp_location_signs(record_id, keyword, need_location) VALUES(:record_id, :keyword, :need_location)",
                named_params! {
                    ":record_id": record_id.as_str(),
                    ":keyword": sign.keyword,
                    ":need_location": sign.need_location,
                },
            )?;
        }

        self.scope.err_if_interrupted()?;
        self.conn.execute_cached(
            "INSERT INTO yelp_custom_details(record_id, icon_id, score) VALUES(:record_id, :icon_id, :score)",
            named_params! {
                ":record_id": record_id.as_str(),
                ":icon_id": suggestion.icon_id,
                ":score": suggestion.score,
            },
        )?;

        Ok(())
    }

    /// Fetch Yelp suggestion from given user's query.
    pub(crate) fn fetch_yelp_suggestions(
        &self,
        query: &SuggestionQuery,
    ) -> Result<Vec<Suggestion>> {
        if !query.providers.contains(&SuggestionProvider::Yelp) {
            return Ok(vec![]);
        }

        if query.keyword.len() > MAX_QUERY_LENGTH {
            return Ok(vec![]);
        }

        let mut query_string = query.keyword.trim();

        let pre_yelp_modifier =
            self.find_modifier(query_string, Modifier::Yelp, FindFrom::First)?;
        if let Some(ref words) = pre_yelp_modifier {
            query_string = query_string[words.len()..].trim();
        }

        let pre_modifier = self.find_modifier(query_string, Modifier::Pre, FindFrom::First)?;
        if let Some(ref words) = pre_modifier {
            query_string = query_string[words.len()..].trim();
        }

        let subject_tuple = self.find_subject(query_string)?;
        if let Some((_, ref matched)) = subject_tuple {
            query_string = query_string[matched.len()..].trim();
        } else {
            return Ok(vec![]);
        }

        let post_modifier = self.find_modifier(query_string, Modifier::Post, FindFrom::First)?;
        if let Some(ref words) = post_modifier {
            query_string = query_string[words.len()..].trim();
        }

        let location_sign = self.find_location_sign(query_string)?;
        if let Some(ref words) = location_sign {
            query_string = query_string[words.len()..].trim();
        }

        let post_yelp_modifier =
            self.find_modifier(query_string, Modifier::Yelp, FindFrom::Last)?;
        if let Some(ref words) = post_yelp_modifier {
            query_string = query_string[..query_string.len() - words.len()].trim();
        }

        let location = if query_string.is_empty() {
            None
        } else {
            Some(query_string.to_string())
        };

        let (icon, icon_mimetype, score) = self.fetch_custom_details()?;
        let subject = subject_tuple.unwrap();
        let builder = SuggestionBuilder {
            subject: &subject.0,
            subject_exact_match: subject.0 == subject.1,
            pre_modifier,
            post_modifier,
            need_location: location_sign.is_some() || location.is_some(),
            location_sign,
            location,
            icon,
            icon_mimetype,
            score,
        };
        Ok(vec![builder.into()])
    }

    /// Find the modifier for given query and modifier type.
    /// Find from last word, if set FindFrom::Last to find_from.
    /// It returns Option<String> that includes the found modifier.
    fn find_modifier(
        &self,
        query: &str,
        modifier_type: Modifier,
        find_from: FindFrom,
    ) -> Result<Option<String>> {
        if query.is_empty() {
            return Ok(None);
        }

        let words: Vec<_> = query.split_whitespace().collect();

        for n in (1..=MAX_MODIFIER_WORDS_NUMBER).rev() {
            let mut candidate_chunks: Box<dyn Iterator<Item = &[&str]>> = match find_from {
                FindFrom::First => Box::new(words.chunks(n)),
                _ => Box::new(words.rchunks(n)),
            };
            let candidate = candidate_chunks.next().unwrap_or(&[""]).join(" ");
            if self.is_modifier(&candidate, modifier_type)? {
                return Ok(Some(candidate));
            }
        }

        Ok(None)
    }

    fn is_modifier(&self, word: &str, modifier_type: Modifier) -> Result<bool> {
        Ok(self.conn.query_row_and_then_cachable(
            "
                SELECT EXISTS (
                    SELECT 1 FROM yelp_modifiers WHERE type = :type AND keyword = :word LIMIT 1
                )
                ",
            named_params! {
                ":type": modifier_type,
                ":word": word.to_lowercase(),
            },
            |row| row.get::<_, bool>(0),
            true,
        )?)
    }

    /// Find the subject for given.
    /// It returns Option<tuple> as follows:
    /// (
    ///   String: The keyword in DB (but the case is inherited by query).
    ///   String: The query that was matched with the keyword.
    /// )
    fn find_subject(&self, query: &str) -> Result<Option<(String, String)>> {
        if query.is_empty() {
            return Ok(None);
        }

        if let Ok(keyword_lowercase) = self.conn.query_row_and_then_cachable(
            "SELECT keyword
             FROM yelp_subjects
             WHERE :query || ' ' LIKE keyword || ' %'
             ORDER BY LENGTH(keyword) ASC, keyword ASC
             LIMIT 1",
            named_params! {
                ":query": query.to_lowercase(),
            },
            |row| row.get::<_, String>(0),
            true,
        ) {
            let keyword = &query[0..keyword_lowercase.len()];
            return Ok(Some((keyword.to_string(), keyword.to_string())));
        };

        if query.len() < SUBJECT_PREFIX_MATCH_THRESHOLD {
            return Ok(None);
        }

        if let Ok(keyword_lowercase) = self.conn.query_row_and_then_cachable(
            "SELECT keyword
             FROM yelp_subjects
             WHERE keyword LIKE :query || '%'
             ORDER BY LENGTH(keyword) ASC, keyword ASC
             LIMIT 1",
            named_params! {
                ":query": query.to_lowercase(),
            },
            |row| row.get::<_, String>(0),
            true,
        ) {
            let keyword = format!("{}{}", query, &keyword_lowercase[query.len()..]);
            return Ok(Some((keyword.to_string(), query.to_string())));
        };

        Ok(None)
    }

    /// Find the location sign for given query and modifier type.
    /// It returns Option<String> that includes the found location sign.
    fn find_location_sign(&self, query: &str) -> Result<Option<String>> {
        if query.is_empty() {
            return Ok(None);
        }

        let words: Vec<_> = query.split_whitespace().collect();

        for n in (1..=MAX_LOCATOIN_SIGN_WORDS_NUMBER).rev() {
            let mut candidate_chunks = words.chunks(n);
            let candidate = candidate_chunks.next().unwrap_or(&[""]).join(" ");
            if self.is_location_sign(&candidate)? {
                return Ok(Some(candidate));
            }
        }

        Ok(None)
    }

    fn is_location_sign(&self, word: &str) -> Result<bool> {
        Ok(self.conn.query_row_and_then_cachable(
            "
                SELECT EXISTS (
                    SELECT 1 FROM yelp_location_signs WHERE keyword = :word LIMIT 1
                )
                ",
            named_params! {
                ":word": word.to_lowercase(),
            },
            |row| row.get::<_, bool>(0),
            true,
        )?)
    }

    /// Fetch the custom details for Yelp suggestions.
    /// It returns the location tuple as follows:
    /// (
    ///   Option<Vec<u8>>: Icon data. If not found, returns None.
    ///   Option<String>: Mimetype of the icon data. If not found, returns None.
    ///   f64: Reflects score field in the yelp_custom_details table.
    /// )
    ///
    /// Note that there should be only one record in `yelp_custom_details`
    /// as all the Yelp assets are stored in the attachment of a single record
    /// on Remote Settings. The following query will perform a table scan against
    /// `yelp_custom_details` followed by an index search against `icons`,
    /// which should be fine since there is only one record in the first table.
    fn fetch_custom_details(&self) -> Result<(Option<Vec<u8>>, Option<String>, f64)> {
        let result = self.conn.query_row_and_then_cachable(
            r#"
            SELECT
              i.data, i.mimetype, y.score
            FROM
              yelp_custom_details y
            LEFT JOIN
              icons i
              ON y.icon_id = i.id
            LIMIT
              1
            "#,
            (),
            |row| -> Result<_> {
                Ok((
                    row.get::<_, Option<Vec<u8>>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            },
            true,
        )?;

        Ok(result)
    }
}

struct SuggestionBuilder<'a> {
    subject: &'a str,
    subject_exact_match: bool,
    pre_modifier: Option<String>,
    post_modifier: Option<String>,
    location_sign: Option<String>,
    location: Option<String>,
    need_location: bool,
    icon: Option<Vec<u8>>,
    icon_mimetype: Option<String>,
    score: f64,
}

impl<'a> From<SuggestionBuilder<'a>> for Suggestion {
    fn from(builder: SuggestionBuilder<'a>) -> Suggestion {
        // This location sign such the 'near by' needs to add as a description parameter.
        let location_modifier = if !builder.need_location {
            builder.location_sign.as_deref()
        } else {
            None
        };
        let description = [
            builder.pre_modifier.as_deref(),
            Some(builder.subject),
            builder.post_modifier.as_deref(),
            location_modifier,
        ]
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>()
        .join(" ");

        // https://www.yelp.com/search?find_desc={description}&find_loc={location}
        let mut url = String::from("https://www.yelp.com/search?");
        let mut parameters = form_urlencoded::Serializer::new(String::new());
        parameters.append_pair("find_desc", &description);
        if let (Some(location), true) = (&builder.location, builder.need_location) {
            parameters.append_pair("find_loc", location);
        }
        url.push_str(&parameters.finish());

        let title = [
            builder.pre_modifier.as_deref(),
            Some(builder.subject),
            builder.post_modifier.as_deref(),
            builder.location_sign.as_deref(),
            builder.location.as_deref(),
        ]
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>()
        .join(" ");

        Suggestion::Yelp {
            url,
            title,
            icon: builder.icon,
            icon_mimetype: builder.icon_mimetype,
            score: builder.score,
            has_location_sign: location_modifier.is_none() && builder.location_sign.is_some(),
            subject_exact_match: builder.subject_exact_match,
            location_param: "find_loc".to_string(),
        }
    }
}

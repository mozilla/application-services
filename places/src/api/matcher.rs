/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::{
    self,
    types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef},
};
use url::Url;

use db::PlacesDb;
use error::{ErrorKind, Result};

const MAX_RESULTS: usize = 10;

#[derive(Hash, Eq, PartialEq, Clone, Copy)]
pub enum SearchOption {
    EnableActions,
    DisablePrivateActions,
    PrivateWindow,
    UserContextId(i64),
}

/// A matcher returns autocomplete matches for a query string. Given a query
/// and options, a matcher tokenizes the query, passes the tokens to all its
/// registered providers, finds matches, filters them using a set of criteria,
/// and returns them. A provider can be anything that returns URL suggestions:
/// Places history, bookmarks, and keywords, synced tabs, search engine
/// suggestions, and extension keywords.
pub struct Matcher<'conn> {
    conn: &'conn PlacesDb,
}

impl<'conn> Matcher<'conn> {
    /// Synchronously queries all providers for autocomplete matches, given a
    /// query string and options. This isn't cancelable yet; once a search is
    /// started, it can't be interrupted, even if the user moves on (see
    /// https://github.com/mozilla/application-services/issues/265).
    pub fn search<Q: AsRef<str>>(&self, query: Q, options: &[SearchOption]) -> Result<Vec<Match>> {
        /// TODO: Tokenize the query.
        let matches = Vec::new();

        let adaptive = Adaptive::new(query.as_ref(), self.conn, MAX_RESULTS);
        let adaptive_matches = adaptive.search()?;

        let suggestions = Suggestions::new(query.as_ref(), self.conn, MAX_RESULTS);
        let suggestions_matches = suggestions.search()?;

        Ok(matches)
    }
}

/// The match reason specifies why an autocomplete search result matched a
/// query. This can be used to filter and sort matches.
pub enum MatchReason {
    QueryString,
    Path,
    Origin,
    Label,
    PreviousUse,
    Boundary,
    Fuzzy,
    Tags(String),
}

pub enum MatchLabel {
    BookmarkTag,
    Tag,
    Bookmark,
    SwitchTab,
    Extension,
    SearchEngine,
    SearchEngineSuggestion,
    RemoteTab,
    VisitURL,
    SearchEngineFavicon,
    Favicon,
}

pub struct Match {
    /// The URL to autocomplete when the user confirms a match. This is
    /// equivalent to `nsIAutoCompleteResult.getFinalCompleteValueAt`;
    /// we don't implement `display_url`.
    pub url: Url,

    /// The title of the autocomplete entry.
    pub title: String,

    // Merge with `reasons`; these don't need to be separate for the UI.
    pub label: MatchLabel,

    /// The favicon URL.
    pub icon_url: Option<Url>,

    /// A frecency score for this match.
    pub frecency: i64,

    /// A list of reasons why this matched.
    pub reasons: Vec<MatchReason>,
}

impl Match {
    /// Default search behaviors from Desktop: HISTORY, BOOKMARK, OPENPAGE, SEARCHES.
    /// Default match behavior: MATCH_BOUNDARY_ANYWHERE.
    pub fn from_adaptive_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let mut reasons = Vec::new();

        let place_id = row.get_checked::<_, i64>("id")?;
        let url = row.get_checked::<_, String>("url")?;
        let history_title = row.get_checked::<_, Option<String>>("title")?;
        let bookmarked = row.get_checked::<_, String>("bookmarked")?;
        let bookmark_title = row.get_checked::<_, Option<String>>("btitle")?;
        let tags = row.get_checked::<_, Option<String>>("tags")?;
        let frecency = row.get_checked::<_, i64>("frecency")?;

        let title = bookmark_title.or_else(|| history_title).unwrap_or_default();

        let label = if let Some(tags) = tags {
            reasons.push(MatchReason::Tags(tags));
            MatchLabel::BookmarkTag
        } else {
            MatchLabel::Bookmark
        };
        let url = Url::parse(&url).expect("Invalid URL in Places");

        Ok(Self {
            url,
            title,
            label,
            icon_url: None,
            frecency,
            reasons,
        })
    }
}

pub enum MatchBehavior {
    Anywhere = 0,
    BoundaryAnywhere = 1,
}

impl FromSql for MatchBehavior {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        Ok(match value.as_i64()? {
            0 => MatchBehavior::Anywhere,
            1 => MatchBehavior::BoundaryAnywhere,
            _ => Err(FromSqlError::InvalidType)?,
        })
    }
}

impl ToSql for MatchBehavior {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput> {
        Ok(match self {
            MatchBehavior::Anywhere => ToSqlOutput::from(0i64),
            MatchBehavior::BoundaryAnywhere => ToSqlOutput::from(1i64),
        })
    }
}

struct Adaptive<'query, 'conn> {
    query: &'query str,
    conn: &'conn PlacesDb,
    max_results: usize,
    match_behavior: MatchBehavior,
}

impl<'query, 'conn> Adaptive<'query, 'conn> {
    pub fn new(
        query: &'query str,
        conn: &'conn PlacesDb,
        max_results: usize,
    ) -> Adaptive<'query, 'conn> {
        Adaptive::with_behavior(query, conn, max_results, MatchBehavior::BoundaryAnywhere)
    }

    pub fn with_behavior(
        query: &'query str,
        conn: &'conn PlacesDb,
        max_results: usize,
        match_behavior: MatchBehavior,
    ) -> Adaptive<'query, 'conn> {
        Adaptive {
            query,
            conn,
            max_results,
            match_behavior,
        }
    }

    pub fn search(&self) -> Result<Vec<Match>> {
        let mut stmt = self.conn.db.prepare(
            "
            SELECT h.url, h.title,
                   EXISTS(SELECT 1 FROM moz_bookmarks
                          WHERE fk = h.id) AS bookmarked,
                   (SELECT title FROM moz_bookmarks
                    WHERE fk = h.id AND
                          title NOT NULL
                    ORDER BY lastModified DESC
                    LIMIT 1) AS btitle,
                   NULL AS tags,
                   h.visit_count, h.typed, h.id, NULL AS open_count, h.frecency
            FROM (
              SELECT ROUND(MAX(use_count) * (1 + (input = :search_string)), 1) AS rank,
                     place_id
              FROM moz_inputhistory
              WHERE input BETWEEN :search_string AND :search_string || X'FFFF'
              GROUP BY place_id
            ) AS i
            JOIN moz_places h ON h.id = i.place_id
            WHERE AUTOCOMPLETE_MATCH(NULL, h.url,
                                     IFNULL(btitle, h.title), tags,
                                     h.visit_count, h.typed, bookmarked,
                                     NULL, :matchBehavior)
            ORDER BY rank DESC, h.frecency DESC
            LIMIT :maxResults
        ",
        )?;
        let params: &[(&str, &dyn rusqlite::types::ToSql)] = &[
            (":search_string", &self.query),
            (":matchBehavior", &self.match_behavior),
            (":maxResults", &(self.max_results as i64)),
        ];
        let mut results = Vec::new();
        for result in stmt.query_and_then_named(params, Match::from_adaptive_row)? {
            results.push(result?);
        }
        Ok(results)
    }
}

struct Suggestions<'query, 'conn> {
    query: &'query str,
    conn: &'conn PlacesDb,
    max_results: usize,
    match_behavior: MatchBehavior,
}

impl<'query, 'conn> Suggestions<'query, 'conn> {
    pub fn new(
        query: &'query str,
        conn: &'conn PlacesDb,
        max_results: usize,
    ) -> Suggestions<'query, 'conn> {
        Suggestions::with_behavior(query, conn, max_results, MatchBehavior::BoundaryAnywhere)
    }

    pub fn with_behavior(
        query: &'query str,
        conn: &'conn PlacesDb,
        max_results: usize,
        match_behavior: MatchBehavior,
    ) -> Suggestions<'query, 'conn> {
        Suggestions {
            query,
            conn,
            max_results,
            match_behavior,
        }
    }

    pub fn search(&self) -> Result<Vec<Match>> {
        let mut results = Vec::new();

        let mut stmt = self.conn.db.prepare(
            "
            SELECT h.url, h.title,
                   EXISTS(SELECT 1 FROM moz_bookmarks
                          WHERE fk = h.id) AS bookmarked,
                   (SELECT title FROM moz_bookmarks
                    WHERE fk = h.id AND
                          title NOT NULL
                    ORDER BY lastModified DESC
                    LIMIT 1) AS btitle,
                   NULL AS tags,
                   h.visit_count, h.typed, h.id, NULL AS open_count, h.frecency
            FROM moz_places h
            WHERE h.frecency <> 0
              AND CASE WHEN bookmarked
                THEN
                  AUTOCOMPLETE_MATCH(:searchString, h.url,
                                     IFNULL(btitle, h.title), tags,
                                     h.visit_count, h.typed,
                                     1, NULL,
                                     :matchBehavior)
                ELSE
                  AUTOCOMPLETE_MATCH(:searchString, h.url,
                                     h.title, '',
                                     h.visit_count, h.typed,
                                     0, NULL,
                                     :matchBehavior)
                END
              AND +h.visit_count > 0
              AND bookmarked
            ORDER BY h.frecency DESC, h.id DESC
            LIMIT :maxResults
        ",
        )?;

        Ok(results)
    }
}

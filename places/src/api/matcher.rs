/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::{
    self,
    types::{FromSql, FromSqlError, FromSqlResult, Null, ToSql, ToSqlOutput, ValueRef},
};
use url::Url;

use db::PlacesDb;
use error::{ErrorKind, Result};

const MAX_RESULTS: usize = 10;

pub fn split_after_prefix(href: &str) -> (&str, &str) {
    match href.find(':') {
        None => ("", href),
        Some(index) => {
            let mut end = index + 1;
            if href.len() >= end + 2 && &href[end..end + 2] == "//" {
                end += 2;
            }
            (&href[0..end], &href[end..])
        }
    }
}

pub fn split_after_host_and_port(href: &str) -> (&str, &str) {
    let (_, remainder) = split_after_prefix(href);
    let mut start = 0;
    let mut end = remainder.len();
    for (index, c) in remainder.chars().enumerate() {
        if c == '/' || c == '?' || c == '#' {
            end = index;
            break;
        }
        if c == '@' {
            start = index + 1;
        }
    }
    (&remainder[start..end], &remainder[end..])
}

fn looks_like_origin(string: &str) -> bool {
    return !string.is_empty() && !string.chars().any(|c|
        c.is_whitespace() || c == '/' || c == '?' || c == '#'
    );
}

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
        // TODO: Tokenize the query.
        let matches = Vec::new();

        // Try to find the first heuristic result. Desktop tries extensions,
        // search engine aliases, Places keywords, origins, URLs, search
        // engine domains, and preloaded sites, before trying to fall back
        // to fixing up the URL, and a search if all else fails. We only try
        // keywords, origins, and URLs, to keep things simple.

        // Try to match on the origin, or the full URL.
        let origin_or_url = OriginOrURL::new(query.as_ref(), self.conn);
        let origin_or_url_matches = origin_or_url.search()?;

        // After the first result, try the queries for adaptive matches and
        // suggestions for bookmarked URLs.
        let adaptive = Adaptive::new(query.as_ref(), self.conn, MAX_RESULTS);
        let adaptive_matches = adaptive.search()?;

        let suggestions = Suggestions::new(query.as_ref(), self.conn, MAX_RESULTS);
        let suggestions_matches = suggestions.search()?;

        // TODO: If we don't have enough results, re-run `Adaptive` and
        // `Suggestions`, this time with `MatchBehavior::Anywhere`.

        Ok(matches)
    }

    /// Records an accepted autocomplete match, recording the query string,
    /// and chosen URL for subsequent matches.
    pub fn accept<Q: AsRef<str>>(&self, query: Q, m: &Match) -> Result<()> {
        // See `nsNavHistory::AutoCompleteFeedback`.
        let mut stmt = self.conn.db.prepare("
            INSERT OR REPLACE INTO moz_inputhistory(place_id, input, use_count)
            SELECT h.id, IFNULL(i.input, :input_text), IFNULL(i.use_count, 0) * .9 + 1
            FROM moz_places h
            LEFT JOIN moz_inputhistory i ON i.place_id = h.id AND i.input = :input_text
            WHERE url_hash = hash(:page_url) AND url = :page_url
        ")?;
        let params: &[(&str, &dyn rusqlite::types::ToSql)] = &[
            (":input_text", &query.as_ref()),
            (":page_url", &m.url.as_str()),
        ];
        stmt.execute_named(params)?;
        Ok(())
    }
}

/// The match reason specifies why an autocomplete search result matched a
/// query. This can be used to filter and sort matches.
pub enum MatchReason {
    Keyword,
    Origin,
    URL,
    PreviousUse,
    Bookmark,
    Tags(String),
}

pub struct Match {
    /// The URL to open when the user confirms a match. This is
    /// equivalent to `nsIAutoCompleteResult.getFinalCompleteValueAt`.
    pub url: Url,

    /// The title of the autocompleted value, to show in the UI. This can be the
    /// title of the bookmark or page, origin, URL, or URL fragment.
    pub title: String,

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
        let mut reasons = vec![MatchReason::PreviousUse];

        let place_id = row.get_checked::<_, i64>("id")?;
        let url = row.get_checked::<_, String>("url")?;
        let history_title = row.get_checked::<_, Option<String>>("title")?;
        let bookmarked = row.get_checked::<_, bool>("bookmarked")?;
        let bookmark_title = row.get_checked::<_, Option<String>>("btitle")?;
        let frecency = row.get_checked::<_, i64>("frecency")?;

        let title = bookmark_title.or_else(|| history_title).unwrap_or_default();

        let tags = row.get_checked::<_, Option<String>>("tags")?;
        if let Some(tags) = tags {
            reasons.push(MatchReason::Tags(tags));
        }
        if bookmarked {
            reasons.push(MatchReason::Bookmark);
        }
        let url = Url::parse(&url).expect("Invalid URL in Places");

        Ok(Self {
            url,
            title,
            icon_url: None,
            frecency,
            reasons,
        })
    }

    pub fn from_suggestion_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let mut reasons = vec![MatchReason::Bookmark];

        let url = row.get_checked::<_, String>("url")?;

        let history_title = row.get_checked::<_, Option<String>>("title")?;
        let bookmark_title = row.get_checked::<_, Option<String>>("btitle")?;
        let title = bookmark_title.or_else(|| history_title).unwrap_or_default();

        let tags = row.get_checked::<_, Option<String>>("tags")?;
        if let Some(tags) = tags {
            reasons.push(MatchReason::Tags(tags));
        }
        let url = Url::parse(&url).expect("Invalid URL in Places");

        let frecency = row.get_checked::<_, i64>("frecency")?;

        Ok(Self {
            url,
            title,
            icon_url: None,
            frecency,
            reasons,
        })
    }

    pub fn from_origin_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let url = row.get_checked::<_, String>("url")?;
        let display_url = row.get_checked::<_, String>("displayURL")?;
        let frecency = row.get_checked::<_, i64>("frecency")?;

        let url = Url::parse(&url).expect("Invalid URL in Places");

        Ok(Self {
            url,
            title: display_url,
            icon_url: None,
            frecency,
            reasons: vec![MatchReason::Origin],
        })
    }

    pub fn from_url_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let url = row.get_checked::<_, String>("url")?;
        let display_url = row.get_checked::<_, String>("displayURL")?;
        let frecency = row.get_checked::<_, i64>("frecency")?;
        let bookmarked = row.get_checked::<_, bool>("bookmarked")?;

        let mut reasons = vec![MatchReason::URL];
        if bookmarked {
            reasons.push(MatchReason::Bookmark);
        }

        let url = Url::parse(&url).expect("Invalid URL in Places");

        Ok(Self {
            url,
            title: display_url,
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

struct OriginOrURL<'query, 'conn> {
    query: &'query str,
    conn: &'conn PlacesDb,
}

impl<'query, 'conn> OriginOrURL<'query, 'conn> {
    pub fn new(query: &'query str, conn: &'conn PlacesDb) -> OriginOrURL<'query, 'conn> {
        OriginOrURL { query, conn }
    }

    pub fn search(&self) -> Result<Vec<Match>> {
        let mut results = Vec::new();
        if looks_like_origin(self.query) {
            let mut stmt = self.conn.db.prepare("
                SELECT host || '/' AS url,
                       IFNULL(:prefix, prefix) || moz_origins.host || '/' AS displayURL,
                       frecency,
                       id
                FROM (
                  SELECT host,
                         TOTAL(frecency) AS host_frecency
                  FROM moz_origins
                  WHERE host BETWEEN :searchString AND :searchString || X'FFFF'
                  GROUP BY host
                  HAVING host_frecency >= :frecencyThreshold
                  UNION ALL
                  SELECT host,
                         TOTAL(frecency) AS host_frecency
                  FROM moz_origins
                  WHERE host BETWEEN 'www.' || :searchString AND 'www.' || :searchString || X'FFFF'
                  GROUP BY host
                  HAVING host_frecency >= :frecencyThreshold
                ) AS grouped_hosts
                JOIN moz_origins ON moz_origins.host = grouped_hosts.host
                ORDER BY frecency DESC, id DESC
                LIMIT 1
            ")?;
            let params: &[(&str, &dyn rusqlite::types::ToSql)] = &[
                (":prefix", &Null),
                (":searchString", &self.query),
                (":frecencyThreshold", &0i64),
            ];
            for result in stmt.query_and_then_named(params, Match::from_origin_row)? {
                results.push(result?);
            }
        } else if let Some(end_host) = self.query.find(|c| c == '/' || c == ':' || c == '?') {
            let host = self.query[..end_host].to_owned();
            let mut stmt = self.conn.db.prepare("
                SELECT url,
                       :strippedURL AS displayURL,
                       frecency,
                       foreign_count > 0 AS bookmarked,
                       id
                FROM moz_places
                WHERE rev_host = reverse_host(:host)
                      AND MAX(frecency, 0) >= :frecencyThreshold
                      AND hidden = 0
                      AND strip_prefix_and_userinfo(url) BETWEEN :strippedURL AND :strippedURL || X'FFFF'
                UNION ALL
                SELECT url,
                       :strippedURL AS displayURL,
                       frecency,
                       foreign_count > 0 AS bookmarked,
                       id
                FROM moz_places
                WHERE rev_host = reverse_host(:host) || 'www.'
                      AND MAX(frecency, 0) >= :frecencyThreshold
                      AND hidden = 0
                      AND strip_prefix_and_userinfo(url) BETWEEN 'www.' || :strippedURL AND 'www.' || :strippedURL || X'FFFF'
                ORDER BY frecency DESC, id DESC
                LIMIT 1
            ")?;
            let params: &[(&str, &dyn rusqlite::types::ToSql)] = &[
                (":strippedURL", &self.query),
                (":host", &host),
                (":frecencyThreshold", &0i64),
            ];
            for result in stmt.query_and_then_named(params, Match::from_url_row)? {
                results.push(result?);
            }
        }
        Ok(results)
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
        let mut stmt = self.conn.db.prepare("
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
        ")?;
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
        let mut stmt = self.conn.db.prepare("
            SELECT h.url, h.title,
                   (SELECT title FROM moz_bookmarks
                    WHERE fk = h.id AND
                          title NOT NULL
                    ORDER BY lastModified DESC
                    LIMIT 1) AS btitle,
                   NULL AS tags,
                   h.visit_count, h.typed, h.id, NULL AS open_count, h.frecency
            FROM moz_places h
            WHERE h.frecency <> 0
              AND AUTOCOMPLETE_MATCH(:searchString, h.url,
                                     IFNULL(btitle, h.title), tags,
                                     h.visit_count, h.typed,
                                     1, NULL,
                                     :matchBehavior)
              AND +h.visit_count > 0
              AND EXISTS(SELECT 1 FROM moz_bookmarks
                         WHERE fk = h.id)
            ORDER BY h.frecency DESC, h.id DESC
            LIMIT :maxResults
        ")?;
        let params: &[(&str, &dyn rusqlite::types::ToSql)] = &[
            (":search_string", &self.query),
            (":matchBehavior", &self.match_behavior),
            (":maxResults", &(self.max_results as i64)),
        ];
        let mut results = Vec::new();
        for result in stmt.query_and_then_named(params, Match::from_suggestion_row)? {
            results.push(result?);
        }
        Ok(results)
    }
}

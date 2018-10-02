/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::{
    self,
    types::{FromSql, FromSqlError, FromSqlResult, Null, ToSql, ToSqlOutput, ValueRef},
};
use url::Url;

use db::PlacesDb;
use error::Result;

#[derive(Debug, Clone)]
pub struct SearchParams {
    pub search_string: String,
    pub limit: u32,
}

/// Synchronously queries all providers for autocomplete matches, then filters
/// the matches. This isn't cancelable yet; once a search is started, it can't
/// be interrupted, even if the user moves on (see
/// https://github.com/mozilla/application-services/issues/265).
///
/// A provider can be anything that returns URL suggestions: Places history
/// and bookmarks, synced tabs, search engine suggestions, and search keywords.
pub fn search_frecent(conn: &PlacesDb, params: SearchParams) -> Result<Vec<SearchResult>> {
    // TODO: Tokenize the query.
    let mut matches = Vec::new();

    // Try to find the first heuristic result. Desktop tries extensions,
    // search engine aliases, origins, URLs, search engine domains, and
    // preloaded sites, before trying to fall back to fixing up the URL,
    // and a search if all else fails. We only try origins and URLs for
    // heuristic matches, since that's all we support.

    // Try to match on the origin, or the full URL.
    let origin_or_url = OriginOrURL::new(&params.search_string, conn);
    let origin_or_url_matches = origin_or_url.search()?;
    matches.extend(origin_or_url_matches);

    // After the first result, try the queries for adaptive matches and
    // suggestions for bookmarked URLs.
    let adaptive = Adaptive::new(&params.search_string, conn, params.limit);
    let adaptive_matches = adaptive.search()?;
    matches.extend(adaptive_matches);

    let suggestions = Suggestions::new(&params.search_string, conn, params.limit);
    let suggestions_matches = suggestions.search()?;
    matches.extend(suggestions_matches);

    // TODO: If we don't have enough results, re-run `Adaptive` and
    // `Suggestions`, this time with `MatchBehavior::Anywhere`.

    Ok(matches)
}

/// Records an accepted autocomplete match, recording the query string,
/// and chosen URL for subsequent matches.
pub fn accept_result(conn: &PlacesDb, result: &SearchResult) -> Result<()> {
    // See `nsNavHistory::AutoCompleteFeedback`.
    let mut stmt = conn.db.prepare("
        INSERT OR REPLACE INTO moz_inputhistory(place_id, input, use_count)
        SELECT h.id, IFNULL(i.input, :input_text), IFNULL(i.use_count, 0) * .9 + 1
        FROM moz_places h
        LEFT JOIN moz_inputhistory i ON i.place_id = h.id AND i.input = :input_text
        WHERE url_hash = hash(:page_url) AND url = :page_url
    ")?;
    let params: &[(&str, &dyn rusqlite::types::ToSql)] = &[
        (":input_text", &result.search_string),
        (":page_url", &result.url.as_str()),
    ];
    stmt.execute_named(params)?;
    Ok(())
}


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

/// The match reason specifies why an autocomplete search result matched a
/// query. This can be used to filter and sort matches.
#[derive(Debug, Clone)]
pub enum MatchReason {
    Keyword,
    Origin,
    Url,
    PreviousUse,
    Bookmark,
    Tags(String),
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The search string for this match.
    pub search_string: String,

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

impl SearchResult {
    /// Default search behaviors from Desktop: HISTORY, BOOKMARK, OPENPAGE, SEARCHES.
    /// Default match behavior: MATCH_BOUNDARY_ANYWHERE.
    pub fn from_adaptive_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let mut reasons = vec![MatchReason::PreviousUse];

        let search_string = row.get_checked::<_, String>("searchString")?;
        let _place_id = row.get_checked::<_, i64>("id")?;
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
            search_string,
            url,
            title,
            icon_url: None,
            frecency,
            reasons,
        })
    }

    pub fn from_suggestion_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let mut reasons = vec![MatchReason::Bookmark];

        let search_string = row.get_checked::<_, String>("searchString")?;
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
            search_string,
            url,
            title,
            icon_url: None,
            frecency,
            reasons,
        })
    }

    pub fn from_origin_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let search_string = row.get_checked::<_, String>("searchString")?;
        let url = row.get_checked::<_, String>("url")?;
        let display_url = row.get_checked::<_, String>("displayURL")?;
        let frecency = row.get_checked::<_, i64>("frecency")?;

        let url = Url::parse(&url).expect("Invalid URL in Places");

        Ok(Self {
            search_string,
            url,
            title: display_url,
            icon_url: None,
            frecency,
            reasons: vec![MatchReason::Origin],
        })
    }

    pub fn from_url_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        let search_string = row.get_checked::<_, String>("searchString")?;
        let url = row.get_checked::<_, String>("url")?;
        let display_url = row.get_checked::<_, String>("displayURL")?;
        let frecency = row.get_checked::<_, i64>("frecency")?;
        let bookmarked = row.get_checked::<_, bool>("bookmarked")?;

        let mut reasons = vec![MatchReason::Url];
        if bookmarked {
            reasons.push(MatchReason::Bookmark);
        }

        let url = Url::parse(&url).expect("Invalid URL in Places");

        Ok(Self {
            search_string,
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

    pub fn search(&self) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();
        if looks_like_origin(self.query) {
            let mut stmt = self.conn.db.prepare("
                SELECT IFNULL(:prefix, prefix) || moz_origins.host || '/' AS url,
                       moz_origins.host || '/' AS displayURL,
                       frecency,
                       id,
                       :searchString AS searchString
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
                (":frecencyThreshold", &-1i64),
            ];
            for result in stmt.query_and_then_named(params, SearchResult::from_origin_row)? {
                results.push(result?);
            }
        } else if self.query.contains(|c| c == '/' || c == ':' || c == '?') {
            let (host, stripped_url) = split_after_host_and_port(self.query);
            let mut stmt = self.conn.db.prepare("
                SELECT h.url,
                       :strippedURL AS displayURL,
                       h.frecency,
                       h.foreign_count > 0 AS bookmarked,
                       h.id,
                       :searchString AS searchString
                FROM moz_places h
                JOIN moz_origins o ON o.id = h.origin_id
                WHERE o.rev_host = reverse_host(:host)
                      AND MAX(h.frecency, 0) >= :frecencyThreshold
                      AND h.hidden = 0
                      AND strip_prefix_and_userinfo(h.url) BETWEEN :strippedURL AND :strippedURL || X'FFFF'
                UNION ALL
                SELECT h.url,
                       :strippedURL AS displayURL,
                       h.frecency,
                       h.foreign_count > 0 AS bookmarked,
                       h.id,
                       :searchString AS searchString
                FROM moz_places h
                JOIN moz_origins o ON o.id = h.origin_id
                WHERE o.rev_host = reverse_host(:host) || 'www.'
                      AND MAX(h.frecency, 0) >= :frecencyThreshold
                      AND h.hidden = 0
                      AND strip_prefix_and_userinfo(h.url) BETWEEN 'www.' || :strippedURL AND 'www.' || :strippedURL || X'FFFF'
                ORDER BY h.frecency DESC, h.id DESC
                LIMIT 1
            ")?;
            let params: &[(&str, &dyn rusqlite::types::ToSql)] = &[
                (":searchString", &self.query),
                (":strippedURL", &stripped_url),
                (":host", &host),
                (":frecencyThreshold", &-1i64),
            ];
            for result in stmt.query_and_then_named(params, SearchResult::from_url_row)? {
                results.push(result?);
            }
        }
        Ok(results)
    }
}

struct Adaptive<'query, 'conn> {
    query: &'query str,
    conn: &'conn PlacesDb,
    max_results: u32,
    match_behavior: MatchBehavior,
}

impl<'query, 'conn> Adaptive<'query, 'conn> {
    pub fn new(
        query: &'query str,
        conn: &'conn PlacesDb,
        max_results: u32,
    ) -> Adaptive<'query, 'conn> {
        Adaptive::with_behavior(query, conn, max_results, MatchBehavior::BoundaryAnywhere)
    }

    pub fn with_behavior(
        query: &'query str,
        conn: &'conn PlacesDb,
        max_results: u32,
        match_behavior: MatchBehavior,
    ) -> Adaptive<'query, 'conn> {
        Adaptive {
            query,
            conn,
            max_results,
            match_behavior,
        }
    }

    pub fn search(&self) -> Result<Vec<SearchResult>> {
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
                   h.visit_count_local + h.visit_count_remote AS visit_count, h.typed,
                   h.id, NULL AS open_count, h.frecency,
                   :searchString AS searchString
            FROM (
              SELECT ROUND(MAX(use_count) * (1 + (input = :searchString)), 1) AS rank,
                     place_id
              FROM moz_inputhistory
              WHERE input BETWEEN :searchString AND :searchString || X'FFFF'
              GROUP BY place_id
            ) AS i
            JOIN moz_places h ON h.id = i.place_id
            WHERE AUTOCOMPLETE_MATCH(:searchString, h.url,
                                     IFNULL(btitle, h.title), tags,
                                     visit_count, h.typed, bookmarked,
                                     NULL, :matchBehavior)
            ORDER BY rank DESC, h.frecency DESC
            LIMIT :maxResults
        ")?;
        let params: &[(&str, &dyn rusqlite::types::ToSql)] = &[
            (":searchString", &self.query),
            (":matchBehavior", &self.match_behavior),
            (":maxResults", &self.max_results),
        ];
        let mut results = Vec::new();
        for result in stmt.query_and_then_named(params, SearchResult::from_adaptive_row)? {
            results.push(result?);
        }
        Ok(results)
    }
}

struct Suggestions<'query, 'conn> {
    query: &'query str,
    conn: &'conn PlacesDb,
    max_results: u32,
    match_behavior: MatchBehavior,
}

impl<'query, 'conn> Suggestions<'query, 'conn> {
    pub fn new(
        query: &'query str,
        conn: &'conn PlacesDb,
        max_results: u32,
    ) -> Suggestions<'query, 'conn> {
        Suggestions::with_behavior(query, conn, max_results, MatchBehavior::BoundaryAnywhere)
    }

    pub fn with_behavior(
        query: &'query str,
        conn: &'conn PlacesDb,
        max_results: u32,
        match_behavior: MatchBehavior,
    ) -> Suggestions<'query, 'conn> {
        Suggestions {
            query,
            conn,
            max_results,
            match_behavior,
        }
    }

    pub fn search(&self) -> Result<Vec<SearchResult>> {
        let mut stmt = self.conn.db.prepare("
            SELECT h.url, h.title,
                   (SELECT title FROM moz_bookmarks
                    WHERE fk = h.id AND
                          title NOT NULL
                    ORDER BY lastModified DESC
                    LIMIT 1) AS btitle,
                   NULL AS tags,
                   h.visit_count_local + h.visit_count_remote AS visit_count, h.typed, h.id,
                   NULL AS open_count, h.frecency, :searchString AS searchString
            FROM moz_places h
            WHERE h.frecency > 0
              AND AUTOCOMPLETE_MATCH(:searchString, h.url,
                                     IFNULL(btitle, h.title), tags,
                                     visit_count, h.typed,
                                     1, NULL,
                                     :matchBehavior)
              AND (+h.visit_count_local > 0 OR +h.visit_count_remote > 0)
            ORDER BY h.frecency DESC, h.id DESC
            LIMIT :maxResults
        ")?;
        let params: &[(&str, &dyn rusqlite::types::ToSql)] = &[
            (":searchString", &self.query),
            (":matchBehavior", &self.match_behavior),
            (":maxResults", &self.max_results),
        ];
        let mut results = Vec::new();
        for result in stmt.query_and_then_named(params, SearchResult::from_suggestion_row)? {
            results.push(result?);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use observation::{VisitObservation};
    use storage::{apply_observation};
    use types::{Timestamp, VisitTransition};

    #[test]
    fn split() {
        assert_eq!(split_after_prefix("http://example.com"), ("http://", "example.com"));
        assert_eq!(split_after_prefix("foo:example"), ("foo:", "example"));
        assert_eq!(split_after_prefix("foo:"), ("foo:", ""));
        assert_eq!(split_after_prefix("notaspec"), ("", "notaspec"));
        assert_eq!(split_after_prefix("http:/"), ("http:", "/"));
        assert_eq!(split_after_prefix("http://"), ("http://", ""));

        assert_eq!(split_after_host_and_port("http://example.com/"), ("example.com", "/"));
        assert_eq!(split_after_host_and_port("http://example.com:8888/"), ("example.com:8888", "/"));
        assert_eq!(split_after_host_and_port("http://user:pass@example.com/"), ("example.com", "/"));
        assert_eq!(split_after_host_and_port("foo:example"), ("example", ""));
    }

    #[test]
    fn search() {
        let mut conn = PlacesDb::open_in_memory(None).expect("no memory db");

        let url = Url::parse("http://example.com/123").unwrap();
        let visit = VisitObservation::new(url.clone())
                   .with_title("Example page 123".to_string())
                   .with_visit_type(VisitTransition::Typed)
                   .with_at(Timestamp::now());

        apply_observation(&mut conn, visit).expect("Should apply visit");

        let by_origin = search_frecent(&conn, SearchParams {
            search_string: "example.com".into(),
            limit: 10,
        }).expect("Should search by origin");
        println!("Matches by origin: {:?}", by_origin);

        let by_url = search_frecent(&conn, SearchParams {
            search_string: "http://example.com".into(),
            limit: 10,
        }).expect("Should search by URL");
        println!("Matches by URL: {:?}", by_url);

        accept_result(&conn, &SearchResult {
            search_string: "ample".into(),
            url: url.clone(),
            title: "Example page 123".into(),
            icon_url: None,
            frecency: -1,
            reasons: vec![],
        }).expect("Should accept input history match");
        let by_adaptive = search_frecent(&conn, SearchParams {
            search_string: "ample".into(),
            limit: 10,
        }).expect("Should search by adaptive input history");
        println!("Matches by adaptive input history: {:?}", by_adaptive);
    }
}

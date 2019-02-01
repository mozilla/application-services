/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::db::PlacesDb;
use crate::error::Result;
pub use crate::match_impl::{MatchBehavior, SearchBehavior};
use serde_derive::*;
use sql_support::ConnExt;
use url::Url;

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

    // Try to find the first heuristic result. Desktop tries extensions,
    // search engine aliases, origins, URLs, search engine domains, and
    // preloaded sites, before trying to fall back to fixing up the URL,
    // and a search if all else fails. We only try origins and URLs for
    // heuristic matches, since that's all we support.

    let matches = match_with_limit(
        &[
            // Try to match on the origin, or the full URL.
            &OriginOrUrl::new(&params.search_string, conn),
            // query adaptive matches and suggestions, matching Anywhere.
            &Adaptive::with_behavior(
                &params.search_string,
                conn,
                MatchBehavior::Anywhere,
                SearchBehavior::default(),
            ),
            &Suggestions::with_behavior(
                &params.search_string,
                conn,
                MatchBehavior::Anywhere,
                SearchBehavior::default(),
            ),
        ],
        params.limit,
    )?;

    Ok(matches)
}

pub fn match_url(conn: &PlacesDb, query: impl AsRef<str>) -> Result<Option<String>> {
    let matcher = OriginOrUrl::new(query.as_ref(), conn);
    // Note: The matcher ignores the limit argument (it's a trait method)
    let results = matcher.search(1)?;
    // Doing it like this lets us move the result, avoiding a copy (which almost
    // certainly doesn't matter but whatever)
    if let Some(res) = results.into_iter().next() {
        Ok(Some(res.url.into_string()))
    } else {
        Ok(None)
    }
}

fn match_with_limit(matchers: &[&dyn Matcher], max_results: u32) -> Result<(Vec<SearchResult>)> {
    let mut results = Vec::new();
    let mut rem_results = max_results;
    for m in matchers {
        if rem_results == 0 {
            break;
        }
        let matches = m.search(rem_results)?;
        results.extend(matches);
        rem_results = rem_results.saturating_sub(results.len() as u32);
    }
    Ok(results)
}

/// Records an accepted autocomplete match, recording the query string,
/// and chosen URL for subsequent matches.
pub fn accept_result(conn: &PlacesDb, result: &SearchResult) -> Result<()> {
    // See `nsNavHistory::AutoCompleteFeedback`.
    conn.execute_named(
        "INSERT OR REPLACE INTO moz_inputhistory(place_id, input, use_count)
         SELECT h.id, IFNULL(i.input, :input_text), IFNULL(i.use_count, 0) * .9 + 1
         FROM moz_places h
         LEFT JOIN moz_inputhistory i ON i.place_id = h.id AND i.input = :input_text
         WHERE url_hash = hash(:page_url) AND url = :page_url",
        &[
            (":input_text", &result.search_string),
            (":page_url", &result.url.as_str()),
        ],
    )?;

    Ok(())
}

pub fn split_after_prefix(href: &str) -> (&str, &str) {
    match memchr::memchr(b':', href.as_bytes()) {
        None => ("", href),
        Some(index) => {
            let hb = href.as_bytes();
            let mut end = index + 1;
            if hb.len() >= end + 2 && hb[end] == b'/' && hb[end + 1] == b'/' {
                end += 2;
            }
            (&href[0..end], &href[end..])
        }
    }
}

pub fn split_after_host_and_port(href: &str) -> (&str, &str) {
    let (_, remainder) = split_after_prefix(href);
    let start = memchr::memchr(b'@', remainder.as_bytes())
        .map(|i| i + 1)
        .unwrap_or(0);
    let remainder = &remainder[start..];
    let end = memchr::memchr3(b'/', b'?', b'#', remainder.as_bytes()).unwrap_or(remainder.len());
    remainder.split_at(end)
}

fn looks_like_origin(string: &str) -> bool {
    // Skip nonascii characters, we'll either handle them in autocomplete_match or,
    // a later part of the origins query.
    !string.is_empty()
        && !string.bytes().any(|c| {
            !c.is_ascii() || c.is_ascii_whitespace() || c == b'/' || c == b'?' || c == b'#'
        })
}

/// The match reason specifies why an autocomplete search result matched a
/// query. This can be used to filter and sort matches.
#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
pub enum MatchReason {
    Keyword,
    Origin,
    Url,
    PreviousUse,
    Bookmark,
    // Hrm... This will probably make this all serialize weird...
    Tags(String),
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
pub struct SearchResult {
    /// The search string for this match.
    pub search_string: String,

    /// The URL to open when the user confirms a match. This is
    /// equivalent to `nsIAutoCompleteResult.getFinalCompleteValueAt`.
    #[serde(with = "url_serde")]
    pub url: Url,

    /// The title of the autocompleted value, to show in the UI. This can be the
    /// title of the bookmark or page, origin, URL, or URL fragment.
    pub title: String,

    /// The favicon URL.
    #[serde(with = "url_serde")]
    #[serde(skip_serializing_if = "Option::is_none")]
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
        let href = row.get_checked::<_, String>("url")?;
        let stripped_url = row.get_checked::<_, String>("strippedURL")?;
        let frecency = row.get_checked::<_, i64>("frecency")?;
        let bookmarked = row.get_checked::<_, bool>("bookmarked")?;

        let mut reasons = vec![MatchReason::Url];
        if bookmarked {
            reasons.push(MatchReason::Bookmark);
        }

        let (url, display_url) = match href.find(&stripped_url) {
            Some(stripped_url_index) => {
                let stripped_prefix = &href[..stripped_url_index];
                let title = match &href[stripped_url_index + stripped_url.len()..].find('/') {
                    Some(next_slash_index) => {
                        &href[stripped_url_index
                            ..=stripped_url_index + stripped_url.len() + next_slash_index]
                    }
                    None => &href[stripped_url_index..],
                };
                let url = Url::parse(&[stripped_prefix, title].concat())
                    .expect("Malformed suggested URL");
                (url, title.into())
            }
            None => {
                let url = Url::parse(&href).expect("Invalid URL in Places");
                (url, stripped_url)
            }
        };

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

trait Matcher {
    fn search(&self, max_results: u32) -> Result<Vec<SearchResult>>;
}

struct OriginOrUrl<'query, 'conn> {
    query: &'query str,
    conn: &'conn PlacesDb,
}

impl<'query, 'conn> OriginOrUrl<'query, 'conn> {
    pub fn new(query: &'query str, conn: &'conn PlacesDb) -> OriginOrUrl<'query, 'conn> {
        OriginOrUrl { query, conn }
    }
}

const URL_SQL: &'static str = "
    SELECT h.url as url,
            :host || :remainder AS strippedURL,
            h.frecency as frecency,
            h.foreign_count > 0 AS bookmarked,
            h.id as id,
            :searchString AS searchString
    FROM moz_places h
    JOIN moz_origins o ON o.id = h.origin_id
    WHERE o.rev_host = reverse_host(:host)
            AND MAX(h.frecency, 0) >= :frecencyThreshold
            AND h.hidden = 0
            AND strip_prefix_and_userinfo(h.url) BETWEEN strippedURL AND strippedURL || X'FFFF'
    UNION ALL
    SELECT h.url as url,
            :host || :remainder AS strippedURL,
            h.frecency as frecency,
            h.foreign_count > 0 AS bookmarked,
            h.id as id,
            :searchString AS searchString
    FROM moz_places h
    JOIN moz_origins o ON o.id = h.origin_id
    WHERE o.rev_host = reverse_host(:host) || 'www.'
            AND MAX(h.frecency, 0) >= :frecencyThreshold
            AND h.hidden = 0
            AND strip_prefix_and_userinfo(h.url) BETWEEN 'www.' || strippedURL AND 'www.' || strippedURL || X'FFFF'
    ORDER BY h.frecency DESC, h.id DESC
    LIMIT 1
";
const ORIGIN_SQL: &'static str = "
    SELECT IFNULL(:prefix, prefix) || moz_origins.host || '/' AS url,
            moz_origins.host || '/' AS displayURL,
            frecency,
            bookmarked,
            id,
            :searchString AS searchString
    FROM (
        SELECT host,
                TOTAL(frecency) AS host_frecency,
                (SELECT TOTAL(foreign_count) > 0 FROM moz_places
                WHERE moz_places.origin_id = moz_origins.id) AS bookmarked
        FROM moz_origins
        WHERE host BETWEEN :searchString AND :searchString || X'FFFF'
        GROUP BY host
        HAVING host_frecency >= :frecencyThreshold
        UNION ALL
        SELECT host,
                TOTAL(frecency) AS host_frecency,
                (SELECT TOTAL(foreign_count) > 0 FROM moz_places
                WHERE moz_places.origin_id = moz_origins.id) AS bookmarked
        FROM moz_origins
        WHERE host BETWEEN 'www.' || :searchString AND 'www.' || :searchString || X'FFFF'
        GROUP BY host
        HAVING host_frecency >= :frecencyThreshold
    ) AS grouped_hosts
    JOIN moz_origins ON moz_origins.host = grouped_hosts.host
    ORDER BY frecency DESC, id DESC
    LIMIT 1
";

impl<'query, 'conn> Matcher for OriginOrUrl<'query, 'conn> {
    fn search(&self, _: u32) -> Result<Vec<SearchResult>> {
        Ok(if looks_like_origin(self.query) {
            self.conn.query_rows_and_then_named_cached(
                ORIGIN_SQL,
                &[
                    (":prefix", &rusqlite::types::Null),
                    (":searchString", &self.query),
                    (":frecencyThreshold", &-1i64),
                ],
                SearchResult::from_origin_row,
            )?
        } else if self.query.contains(|c| c == '/' || c == ':' || c == '?') {
            let (host, remainder) = split_after_host_and_port(self.query);
            let punycode_host = idna::domain_to_ascii(host).ok();
            let host_str = punycode_host.as_ref().map(|s| s.as_str()).unwrap_or(host);
            self.conn.query_rows_and_then_named_cached(
                URL_SQL,
                &[
                    (":searchString", &self.query),
                    (":host", &host_str),
                    (":remainder", &remainder),
                    (":frecencyThreshold", &-1i64),
                ],
                SearchResult::from_url_row,
            )?
        } else {
            vec![]
        })
    }
}

struct Adaptive<'query, 'conn> {
    query: &'query str,
    conn: &'conn PlacesDb,
    match_behavior: MatchBehavior,
    search_behavior: SearchBehavior,
}

impl<'query, 'conn> Adaptive<'query, 'conn> {
    pub fn with_behavior(
        query: &'query str,
        conn: &'conn PlacesDb,
        match_behavior: MatchBehavior,
        search_behavior: SearchBehavior,
    ) -> Adaptive<'query, 'conn> {
        Adaptive {
            query,
            conn,
            match_behavior,
            search_behavior,
        }
    }
}

impl<'query, 'conn> Matcher for Adaptive<'query, 'conn> {
    fn search(&self, max_results: u32) -> Result<Vec<SearchResult>> {
        Ok(self.conn.query_rows_and_then_named_cached(
            "
            SELECT h.url as url,
                   h.title as title,
                   EXISTS(SELECT 1 FROM moz_bookmarks
                          WHERE fk = h.id) AS bookmarked,
                   (SELECT title FROM moz_bookmarks
                    WHERE fk = h.id AND
                          title NOT NULL
                    ORDER BY lastModified DESC
                    LIMIT 1) AS btitle,
                   NULL AS tags,
                   h.visit_count_local + h.visit_count_remote AS visit_count,
                   h.typed as typed,
                   h.id as id,
                   NULL AS open_count,
                   h.frecency as frecency,
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
                                     NULL, :matchBehavior, :searchBehavior)
            ORDER BY rank DESC, h.frecency DESC
            LIMIT :maxResults",
            &[
                (":searchString", &self.query),
                (":matchBehavior", &self.match_behavior),
                (":searchBehavior", &self.search_behavior),
                (":maxResults", &max_results),
            ],
            SearchResult::from_adaptive_row,
        )?)
    }
}

struct Suggestions<'query, 'conn> {
    query: &'query str,
    conn: &'conn PlacesDb,
    match_behavior: MatchBehavior,
    search_behavior: SearchBehavior,
}

impl<'query, 'conn> Suggestions<'query, 'conn> {
    pub fn with_behavior(
        query: &'query str,
        conn: &'conn PlacesDb,
        match_behavior: MatchBehavior,
        search_behavior: SearchBehavior,
    ) -> Suggestions<'query, 'conn> {
        Suggestions {
            query,
            conn,
            match_behavior,
            search_behavior,
        }
    }
}

impl<'query, 'conn> Matcher for Suggestions<'query, 'conn> {
    fn search(&self, max_results: u32) -> Result<Vec<SearchResult>> {
        Ok(self.conn.query_rows_and_then_named_cached(
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
                   h.visit_count_local + h.visit_count_remote AS visit_count,
                   h.typed as typed,
                   h.id as id,
                   NULL AS open_count, h.frecency, :searchString AS searchString
            FROM moz_places h
            WHERE h.frecency > 0
              AND AUTOCOMPLETE_MATCH(:searchString, h.url,
                                     IFNULL(btitle, h.title), tags,
                                     visit_count, h.typed,
                                     bookmarked, NULL,
                                     :matchBehavior, :searchBehavior)
              AND (+h.visit_count_local > 0 OR +h.visit_count_remote > 0)
            ORDER BY h.frecency DESC, h.id DESC
            LIMIT :maxResults",
            &[
                (":searchString", &self.query),
                (":matchBehavior", &self.match_behavior),
                (":searchBehavior", &self.search_behavior),
                (":maxResults", &max_results),
            ],
            SearchResult::from_suggestion_row,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::VisitObservation;
    use crate::storage::history::apply_observation;
    use crate::types::{Timestamp, VisitTransition};

    #[test]
    fn split() {
        assert_eq!(
            split_after_prefix("http://example.com"),
            ("http://", "example.com")
        );
        assert_eq!(split_after_prefix("foo:example"), ("foo:", "example"));
        assert_eq!(split_after_prefix("foo:"), ("foo:", ""));
        assert_eq!(split_after_prefix("notaspec"), ("", "notaspec"));
        assert_eq!(split_after_prefix("http:/"), ("http:", "/"));
        assert_eq!(split_after_prefix("http://"), ("http://", ""));

        assert_eq!(
            split_after_host_and_port("http://example.com/"),
            ("example.com", "/")
        );
        assert_eq!(
            split_after_host_and_port("http://example.com:8888/"),
            ("example.com:8888", "/")
        );
        assert_eq!(
            split_after_host_and_port("http://user:pass@example.com/"),
            ("example.com", "/")
        );
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

        let by_origin = search_frecent(
            &conn,
            SearchParams {
                search_string: "example.com".into(),
                limit: 10,
            },
        )
        .expect("Should search by origin");
        assert!(by_origin
            .iter()
            .any(|result| result.search_string == "example.com"
                && result.title == "example.com/"
                && result.url.as_str() == "http://example.com/"
                && result.reasons == &[MatchReason::Origin]));

        let by_url_without_path = search_frecent(
            &conn,
            SearchParams {
                search_string: "http://example.com".into(),
                limit: 10,
            },
        )
        .expect("Should search by URL without path");
        assert!(by_url_without_path
            .iter()
            .any(|result| result.title == "example.com/"
                && result.url.as_str() == "http://example.com/"
                && result.reasons == &[MatchReason::Url]));

        let by_url_with_path = search_frecent(
            &conn,
            SearchParams {
                search_string: "http://example.com/1".into(),
                limit: 10,
            },
        )
        .expect("Should search by URL with path");
        assert!(by_url_with_path
            .iter()
            .any(|result| result.title == "example.com/123"
                && result.url.as_str() == "http://example.com/123"
                && result.reasons == &[MatchReason::Url]));

        accept_result(
            &conn,
            &SearchResult {
                search_string: "ample".into(),
                url: url.clone(),
                title: "Example page 123".into(),
                icon_url: None,
                frecency: -1,
                reasons: vec![],
            },
        )
        .expect("Should accept input history match");

        let by_adaptive = search_frecent(
            &conn,
            SearchParams {
                search_string: "ample".into(),
                limit: 10,
            },
        )
        .expect("Should search by adaptive input history");
        assert!(by_adaptive
            .iter()
            .any(|result| result.search_string == "ample"
                && result.url == url
                && result.reasons == &[MatchReason::PreviousUse]));

        let with_limit = search_frecent(
            &conn,
            SearchParams {
                search_string: "example".into(),
                limit: 1,
            },
        )
        .expect("Should search until reaching limit");
        assert_eq!(
            with_limit,
            vec![SearchResult {
                search_string: "example".into(),
                url: Url::parse("http://example.com/").unwrap(),
                title: "example.com/".into(),
                icon_url: None,
                frecency: -1,
                reasons: vec![MatchReason::Origin],
            }]
        );
    }
    #[test]
    fn search_unicode() {
        let mut conn = PlacesDb::open_in_memory(None).expect("no memory db");

        let url = Url::parse("http://exämple.com/123").unwrap();
        let visit = VisitObservation::new(url.clone())
            .with_title("Example page 123".to_string())
            .with_visit_type(VisitTransition::Typed)
            .with_at(Timestamp::now());

        apply_observation(&mut conn, visit).expect("Should apply visit");

        let by_url_without_path = search_frecent(
            &conn,
            SearchParams {
                search_string: "http://exämple.com".into(),
                limit: 10,
            },
        )
        .expect("Should search by URL without path");
        assert!(by_url_without_path
            .iter()
            // Should we consider un-punycoding the title? (firefox desktop doesn't...)
            .any(|result| result.title == "xn--exmple-cua.com/"
                && result.url.as_str() == "http://xn--exmple-cua.com/"
                && result.reasons == &[MatchReason::Url]));

        let by_url_with_path = search_frecent(
            &conn,
            SearchParams {
                search_string: "http://exämple.com/1".into(),
                limit: 10,
            },
        )
        .expect("Should search by URL with path");
        assert!(
            by_url_with_path
                .iter()
                .any(|result| result.title == "xn--exmple-cua.com/123"
                    && result.url.as_str() == "http://xn--exmple-cua.com/123"
                    && result.reasons == &[MatchReason::Url]),
            "{:?}",
            by_url_with_path
        );
    }

}

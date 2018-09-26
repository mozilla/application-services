/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Roughly, UnifiedComplete.js's stuff

use url::{Url};
use error::*;
use super::connection::{Connection};
use db::db::MAX_VARIABLE_NUMBER;
use util;
// rusqlite imports probably reflect that some of this should be in ::storage
use rusqlite;

#[derive(Debug, Clone)]
pub struct SearchParams {
    pub search_string: String,
    pub limit: u32
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: Option<String>,
    pub url: Url,
    pub frecency: i64
}

impl SearchResult {
    pub fn from_row(row: &rusqlite::Row) -> Result<Self> {
        Ok(Self {
            title: row.get_checked::<_, Option<String>>("title")?,
            url: Url::parse(&row.get_checked::<_, Option<String>>("url")?.expect("we gotta have a url in the resultset"))?,
            frecency: row.get_checked("frecency")?
        })
    }
}

// There's a lot of boilerplate to return an iterator here (we can't just return
// `impl Iterator<Item = ...> + 'a` because of rusqlite restrictions)
pub fn search_frecent(conn: &Connection, params: SearchParams) -> Result<Vec<SearchResult>> {
    // * result should be an iter of SearchResult...
    // * should have a "::storage" layer and not touch the db directly?
    // * obvs the most trivial impl possible!
    // * etc...

    let tokens = params.search_string.split_whitespace().collect::<Vec<&str>>();
    // Discard tokens beyond MAX_VARIABLE_NUMBER (Hacky, but almost certainly doesn't matter)
    let token_slice = &tokens[..tokens.len().min(MAX_VARIABLE_NUMBER - 1)];

    let tokens_as_tosql = token_slice.iter()
        .map(|t| t as &dyn rusqlite::types::ToSql)
        .collect::<Vec<_>>();

    let sql = format!("
        SELECT url, title, frecency
        FROM moz_places
        WHERE {clause}
        ORDER BY frecency DESC
        LIMIT {max}
    ", clause = util::repeat_display(tokens.len(), " AND ", |i, f|
            write!(f, "(INSTR(url, ?{var_num}) > 0
                        OR (title IS NOT NULL AND
                            INSTR(title, ?{var_num}) > 0))",
                   var_num = i + 1)),
       max = params.limit);

    let conn_db = conn.get_db();
    let mut stmt = conn_db.db.prepare(&sql)?;
    // Couldn't make this work as a collect (probably because of all the Result<>)
    let mut result = vec![];
    for res_row in stmt.query_map(&tokens_as_tosql, SearchResult::from_row)? {
        // First ? is rusqlite errors, second is errors in SearchResult::from_row.
        let parsed = res_row??;
        result.push(parsed);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::history::*;
    use super::super::connection::Connection;
    use url::{Url};
    use ::types::*;
    use ::storage::{PageId};
    #[test]
    fn test_dumb_search() {
        let c = Connection::new_in_memory(None).expect("should get a connection");
        let url = Url::parse("http://example.com").expect("it's a valid url");
        let visits = vec![AddableVisit { date: Timestamp::now(),
                                         transition: VisitTransition::Link,
                                         referrer: None,
                                         is_local: true}];
        let a = AddablePlaceInfo { page_id: PageId::Url(url), title: None, visits };

        insert(&c, a).expect("should insert");

        // phew - finally we can search
        let maybe = search_frecent(&c, SearchParams { search_string: "exam".into(), limit: 2 }).expect("must have worked");
        let result = maybe.get(0).expect("should have actually matched too!");
        assert_eq!(result.url.to_string(), "http://example.com/"); // hrmph - note trailing slash
    }
}

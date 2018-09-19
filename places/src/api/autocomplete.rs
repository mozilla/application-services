/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Roughly, UnifiedComplete.js's stuff

use url::{Url};
use error::*;
use super::connection::{Connection};

// rusqlite imports probably reflect that some of this should be in ::storage
use rusqlite::{Row};

#[derive(Debug)]
pub struct SearchParams {
    search_string: String,
}

impl SearchParams {
    // Can't work out how to make this work :/
    /*
    fn named_params(&self) -> &[(&str, &ToSql)] {
        &[(":searchString", &self.search_string)]
    }
    */
}

#[derive(Debug)]
pub struct SearchResult {
    url: Url,
}

impl SearchResult {
    pub fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            url: Url::parse(&row.get_checked::<_, Option<String>>("url")?.expect("we gotta have a url in the resultset"))?,
        })
    }
}

pub fn search_frecent(conn: &Connection, params: SearchParams) /* -> iter of SearchResult */ -> Result<Option<SearchResult>> {
    // * result should be an iter of SearchResult...
    // * should have a "::storage" layer and not touch the db directly?
    // * obvs the most trivial impl possible!
    // * etc...
    let sql = "SELECT url from moz_places where url like :searchString";
    // let params = params.named_params();
    Ok(conn.get_db().query_row_named(sql, &[(":searchString", &params.search_string)], SearchResult::from_row)?)
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
        let maybe = search_frecent(&c, SearchParams { search_string: "%exam%".into() }).expect("must have worked");
        let result = maybe.expect("should have actually matched too!");
        assert_eq!(result.url.into_string(), "http://example.com/"); // hrmph - note trailing slash
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use url::{Url};

use error::*;
use types::*;
use super::connection::{Connection};
use ::storage::{PageId, fetch_page_info, new_page_info, NewPageInfo, add_visit};

// This module can become, roughly: PlacesUtils.history()

// eg: PlacesUtils.history.insert({url: "http", title: ..., visits: [{date: ...}]})

// Structs representing place and visit infos for this API.
// (Not clear this makes sense - it's a copy of what desktop does just to
// get started)
#[derive(Debug)]
pub struct AddablePlaceInfo {
    page_id: PageId,
    title: Option<String>,
    visits: Vec<AddableVisit>,
}

#[derive(Debug)]
pub struct AddableVisit {
    date: Timestamp,
    transition: VisitTransition,
    referrer: Option<Url>,
    is_local: bool,
}

// insert a visit.
pub fn insert(conn: &Connection, place: AddablePlaceInfo) -> Result<()> {
    // This is roughly what desktop does via js -> History.cpp
    // XXX - needs a transaction.
    //let place = place.normalize()?;
    // If we don't already have the page, we must create it. If we do already
    // have the page we may need to update it.
    let place_row_id = match fetch_page_info(&conn.get_db(), &place.page_id)? {
        Some(existing) => existing.row_id,
        None => {
            // Add a new place. To create a record we must have a url rather than a guid.
            let url = match place.page_id {
                PageId::Url(url) => url,
                _ => panic!("hrm - not sure PageId was a great idea after all?"),
            };
            let pi = NewPageInfo { url, title: place.title, hidden: false, typed: 0 };
            let (_, row_id) = new_page_info(&conn.get_db(), &pi)?;
            row_id
        }
    };
    for v in place.visits {
        add_visit(&conn.get_db(), &place_row_id, &None, &v.date, &v.transition, &v.is_local)?;
    }
    // Recalc frecency. Referrers. Other stuff :(
    // Possibly update place, not clear yet :)
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::connection::Connection;

    #[test]
    fn test_insert() {
        let c = Connection::new_in_memory(None).expect("should get a connection");
        let url = Url::parse("http://example.com").expect("it's a valid url");
        let date = Timestamp::now();
        let visits = vec![AddableVisit { date,
                                         transition: VisitTransition::Link,
                                         referrer: None,
                                         is_local: true}];
        let a = AddablePlaceInfo { page_id: PageId::Url(url), title: None, visits };

        insert(&c, a).expect("should insert");

        // For now, a raw read of the DB.
        let sql = "SELECT p.id, p.url, p.title,
                          p.visit_count_local, p.visit_count_remote,
                          p.hidden, p.typed, p.frecency,
                          p.last_visit_date_local, p.last_visit_date_remote,
                          p.guid, p.foreign_count, p.url_hash, p.description,
                          p.preview_image_url, p.origin_id,
                          v.is_local, v.from_visit, v.place_id,
                          v.visit_date, v.visit_type
                    FROM moz_places p, moz_historyvisits v
                    WHERE v.place_id = p.id";

        let mut stmt = c.get_db().db.prepare(sql).expect("valid sql");
        let mut rows = stmt.query(&[]).expect("should execute");
        let result = rows.next().expect("should get a row");
        let row = result.expect("expect anything");

        assert_eq!(row.get::<_, String>("url"), "http://example.com/"); // hrmph - note trailing slash
        assert_eq!(row.get::<_, Timestamp>("visit_date"), date);
        // XXX - check more.
    }
}

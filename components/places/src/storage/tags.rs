/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{fetch_page_info, TAG_LENGTH_MAX};
use crate::error::{InvalidPlaceInfo, Result};
use rusqlite::Connection;
use sql_support::ConnExt;
use url::Url;

pub fn validate_tag(t: String) -> Result<String> {
    // Drop empty and oversized tags.
    let t = t.trim().to_string();
    if t.len() == 0 || t.len() > TAG_LENGTH_MAX {
        Err(InvalidPlaceInfo::InvalidTag(t).into())
    } else {
        Ok(t)
    }
}

/// TODO: docstrings for all public functions.
pub fn tag_url(conn: &Connection, url: &Url, tag: String) -> Result<()> {
    let tag = validate_tag(tag)?;
    let tx = conn.unchecked_transaction()?;

    // This function will not create a new place.
    // Fetch the place id, so we (a) avoid creating a new tag when we aren't
    // going to reference it and (b) to avoid a sub-query.
    let place_id = match fetch_page_info(conn, url)? {
        Some(info) => info.page.row_id,
        None => return Err(InvalidPlaceInfo::NoItem(url.to_string()).into()),
    };

    conn.execute_named_cached(
        "INSERT OR IGNORE INTO moz_tags(tag, lastModified)
         VALUES(:tag, now())",
        &[(":tag", &tag)],
    )?;

    conn.execute_named_cached(
        "INSERT OR IGNORE INTO moz_tags_relation(tag_id, place_id)
         VALUES((SELECT id FROM moz_tags WHERE tag = :tag), :place_id)",
        &[(":tag", &tag), (":place_id", &place_id)],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn untag_url(conn: &Connection, url: &Url, tag: String) -> Result<()> {
    let tag = validate_tag(tag)?;
    let tx = conn.unchecked_transaction()?;

    conn.execute_named_cached(
        "DELETE FROM moz_tags_relation
         WHERE tag_id = (SELECT id FROM moz_tags
                         WHERE tag = :tag)
         AND place_id = (SELECT id FROM moz_places
                         WHERE url_hash = hash(:url)
                         AND url = :url)",
        &[(":tag", &tag), (":url", &url.as_str())],
    )?;
    // hrmph - do we actually need a transaction here?
    tx.commit()?;
    Ok(())
}

pub fn remove_all_tags_from_url(conn: &Connection, url: &Url) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    conn.execute_named_cached(
        "DELETE FROM moz_tags_relation
         WHERE
         place_id = (SELECT id FROM moz_places
                     WHERE url_hash = hash(:url)
                     AND url = :url)",
        &[(":url", &url.as_str())],
    )?;
    // hrmph - do we actually need a transaction here?
    tx.commit()?;
    Ok(())
}

pub fn remove_tag(conn: &Connection, tag: String) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    conn.execute_named_cached(
        "DELETE FROM moz_tags
         WHERE tag = :tag",
        &[(":tag", &tag)],
    )?;
    // hrmph - do we actually need a transaction here?
    tx.commit()?;
    Ok(())
}

pub fn get_urls_with_tag(conn: &Connection, tag: String) -> Result<Vec<Url>> {
    let tag = validate_tag(tag)?;

    let mut stmt = conn.prepare(
        "SELECT p.url FROM moz_places p
         JOIN moz_tags_relation r ON r.place_id = p.id
         JOIN moz_tags t ON t.id = r.tag_id
         WHERE t.tag = :tag",
    )?;

    let rows =
        stmt.query_and_then_named(&[(":tag", &tag)], |row| row.get_checked::<_, String>("url"))?;
    let mut urls = Vec::new();
    for row in rows {
        urls.push(Url::parse(&row?)?);
    }
    Ok(urls)
}

pub fn get_tags_for_url(conn: &Connection, url: &Url) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.tag
         FROM moz_tags t
         JOIN moz_tags_relation r ON r.tag_id = t.id
         JOIN moz_places h ON h.id = r.place_id
         WHERE url_hash = hash(:url) AND url = :url",
    )?;
    let rows = stmt.query_and_then_named(&[(":url", &url.as_str())], |row| {
        row.get_checked::<_, String>("tag")
    })?;
    let mut tags = Vec::new();
    for row in rows {
        tags.push(row?);
    }
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::test::new_mem_connection;
    use crate::storage::new_page_info;

    fn check_tags_for_url(conn: &Connection, url: &Url, mut expected: Vec<String>) {
        let mut tags = get_tags_for_url(&conn, &url).expect("should work");
        tags.sort();
        expected.sort();
        assert_eq!(tags, expected);
    }

    fn check_urls_with_tag(conn: &Connection, tag: &str, mut expected: Vec<Url>) {
        let mut with_tag = get_urls_with_tag(conn, tag.to_string()).expect("should work");
        with_tag.sort();
        expected.sort();
        assert_eq!(with_tag, expected);
    }

    #[test]
    fn test_tags() {
        let conn = new_mem_connection();
        let url1 = Url::parse("http://example.com").expect("valid url");
        let url2 = Url::parse("http://example2.com").expect("valid url");

        new_page_info(&conn, &url1, None).expect("should create the page");
        new_page_info(&conn, &url2, None).expect("should create the page");
        check_tags_for_url(&conn, &url1, vec![]);
        check_tags_for_url(&conn, &url2, vec![]);

        tag_url(&conn, &url1, "common".to_string()).expect("should work");
        tag_url(&conn, &url1, "tag-1".to_string()).expect("should work");
        tag_url(&conn, &url2, "common".to_string()).expect("should work");
        tag_url(&conn, &url2, "tag-2".to_string()).expect("should work");

        check_tags_for_url(
            &conn,
            &url1,
            vec!["common".to_string(), "tag-1".to_string()],
        );
        check_tags_for_url(
            &conn,
            &url2,
            vec!["common".to_string(), "tag-2".to_string()],
        );

        check_urls_with_tag(&conn, "common", vec![url1.clone(), url2.clone()]);
        check_urls_with_tag(&conn, "tag-1", vec![url1.clone()]);
        check_urls_with_tag(&conn, "tag-2", vec![url2.clone()]);

        untag_url(&conn, &url1, "common".to_string()).expect("should work");

        check_urls_with_tag(&conn, "common", vec![url2.clone()]);

        remove_tag(&conn, "common".to_string()).expect("should work");
        check_urls_with_tag(&conn, "common", vec![]);

        remove_tag(&conn, "tag-1".to_string()).expect("should work");
        check_urls_with_tag(&conn, "tag-1", vec![]);

        remove_tag(&conn, "tag-2".to_string()).expect("should work");
        check_urls_with_tag(&conn, "tag-2", vec![]);

        // should be no tags rows left.
        let count: Result<Option<u32>> = conn.try_query_row(
            "SELECT COUNT(*) from moz_tags",
            &[],
            |row| Ok(row.get_checked::<_, u32>(0)?),
            true,
        );
        assert_eq!(count.unwrap().unwrap(), 0);
        let count: Result<Option<u32>> = conn.try_query_row(
            "SELECT COUNT(*) from moz_tags_relation",
            &[],
            |row| Ok(row.get_checked::<_, u32>(0)?),
            true,
        );
        assert_eq!(count.unwrap().unwrap(), 0);

        // places should still exist.
        fetch_page_info(&conn, &url1)
            .expect("should work")
            .expect("should exist");
        fetch_page_info(&conn, &url2)
            .expect("should work")
            .expect("should exist");
    }

    // TODO - invalid tags and other error conditions.
}

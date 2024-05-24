/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde_json::Value;

use crate::{
    db::PlacesDb,
    storage::bookmarks::get_raw_bookmark,
    storage::bookmarks::json_tree::{fetch_tree, insert_tree, BookmarkTreeNode, FetchDepth},
    types::BookmarkType,
};

use sql_support::ConnExt;
use sync_guid::Guid as SyncGuid;
use types::Timestamp;

use pretty_assertions::assert_eq;

pub fn insert_json_tree(conn: &PlacesDb, jtree: Value) {
    let tree: BookmarkTreeNode = serde_json::from_value(jtree).expect("should be valid");
    let folder_node = match tree {
        BookmarkTreeNode::Folder { f: folder_node } => folder_node,
        _ => panic!("must be a folder"),
    };
    insert_tree(conn, folder_node).expect("should insert");
}

pub struct InvalidBookmarkIds {
    pub place_id: i64,
    pub guid: SyncGuid,
}

// Append a bookmark with an invalid URL to the specified parent. Note that it's
// currently impossible to append a bookmark with NULL as there is a CHECK
// constraint in the schema.
pub fn append_invalid_bookmark(
    db: &PlacesDb,
    parent_guid: &SyncGuid,
    title: &str,
    url: &str,
) -> InvalidBookmarkIds {
    let parent = get_raw_bookmark(db, parent_guid)
        .expect("should work")
        .expect("parent must exist");
    assert_eq!(parent.bookmark_type, BookmarkType::Folder);
    let position = parent.child_count;
    let guid = SyncGuid::random();

    // Assume the invalid URL isn't already there.
    let place_sql = "
        INSERT INTO moz_places (guid, url, url_hash)
        VALUES (:guid, :url, hash(:url))";
    db.execute_cached(
        place_sql,
        &[
            (":guid", &guid as &dyn rusqlite::ToSql),
            (":url", &url.to_string()),
        ],
    )
    .expect("should insert");
    let place_id = db.conn().last_insert_rowid();

    let bm_sql = format!(
        "
        INSERT INTO moz_bookmarks
            (fk, type, parent, position,
             title,  dateAdded, lastModified, guid)
        VALUES
            ({place_id}, {bm_type}, {parent_id}, {position},
             :title, {timestamp}, {timestamp}, :guid)",
        place_id = place_id,
        bm_type = BookmarkType::Bookmark as u8,
        timestamp = Timestamp::now(),
        parent_id = parent.row_id.0,
        position = position,
    );
    db.execute_cached(
        &bm_sql,
        &[
            (":title", &title.to_string() as &dyn rusqlite::ToSql),
            (":guid", &guid),
        ],
    )
    .expect("should insert bookmark");
    InvalidBookmarkIds { place_id, guid }
}

pub fn assert_json_tree(conn: &PlacesDb, folder: &SyncGuid, expected: Value) {
    assert_json_tree_with_depth(conn, folder, expected, &FetchDepth::Deepest)
}

pub fn assert_json_tree_with_depth(
    conn: &PlacesDb,
    folder: &SyncGuid,
    expected: Value,
    target_depth: &FetchDepth,
) {
    let (fetched, _, _) = fetch_tree(conn, folder, target_depth)
        .expect("error fetching tree")
        .unwrap();
    let deser_tree: BookmarkTreeNode = serde_json::from_value(expected).unwrap();
    assert_eq!(fetched, deser_tree);
    // and while checking the tree, check positions are correct.
    check_positions(conn);
}

// check the positions for children in a folder are "correct" in that
// the first child has a value of zero, etc - ie, this will fail if there
// are holes or duplicates in the position values.
// Clever implementation stolen from desktop.
pub fn check_positions(conn: &PlacesDb) {
    // Use triangular numbers to detect skipped position, then
    // a subquery to select enough fields to help diagnose when it fails.
    let sql = "
        WITH bad_parents(pid) as (
            SELECT parent
            FROM moz_bookmarks
            GROUP BY parent
            HAVING (SUM(DISTINCT position + 1) - (count(*) * (count(*) + 1) / 2)) <> 0
        )
        SELECT parent, guid, title, position FROM moz_bookmarks
        WHERE parent in bad_parents
        ORDER BY parent, position
    ";

    let mut stmt = conn.prepare(sql).expect("sql is ok");
    let parents: Vec<_> = stmt
        .query_and_then([], |row| -> rusqlite::Result<_> {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, u32>(3)?,
            ))
        })
        .expect("should work")
        .map(rusqlite::Result::unwrap)
        .collect();

    assert_eq!(parents, Vec::new());
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use rusqlite::{Row, NO_PARAMS};
use serde_json::Value;

use crate::{
    db::PlacesDb,
    error::*,
    storage::{
        bookmarks::{fetch_tree, insert_tree, BookmarkTreeNode},
        RowId,
    },
    types::{SyncGuid, SyncedBookmarkKind, SyncedBookmarkValidity, Timestamp},
};

use pretty_assertions::assert_eq;
use sql_support::{self, ConnExt};
use sync15::ServerTimestamp;
use url::Url;

pub fn insert_json_tree(conn: &PlacesDb, jtree: Value) {
    let tree: BookmarkTreeNode = serde_json::from_value(jtree).expect("should be valid");
    let folder_node = match tree {
        BookmarkTreeNode::Folder(folder_node) => folder_node,
        _ => panic!("must be a folder"),
    };
    insert_tree(conn, &folder_node).expect("should insert");
}

pub fn assert_json_tree(conn: &PlacesDb, folder: &SyncGuid, expected: Value) {
    let fetched = fetch_tree(conn, folder)
        .expect("error fetching tree")
        .unwrap();
    let deser_tree: BookmarkTreeNode = serde_json::from_value(expected).unwrap();
    assert_eq!(fetched, deser_tree);
    // and while checking the tree, check positions are correct.
    check_positions(&conn);
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
        .query_and_then(NO_PARAMS, |row| -> rusqlite::Result<_> {
            Ok((
                row.get_checked::<_, i64>(0)?,
                row.get_checked::<_, String>(1)?,
                row.get_checked::<_, Option<String>>(2)?,
                row.get_checked::<_, u32>(3)?,
            ))
        })
        .expect("should work")
        .map(|v| v.unwrap())
        .collect();

    assert_eq!(parents, Vec::new());
}

// Our prod code never needs to read moz_bookmarks_synced, but our test code
// does.
// MirrorBookmarkValue is used in our struct so that we can do "smart"
// comparisons - if an object created by tests has
// MirrorBookmarkValue::Unspecified, we don't check the value against the
// target of the comparison. We use this instead of Option<> so that we
// can correctly check Option<> fields (ie, so that None isn't ambiguous
// between "no value specified" and "value is exactly None"
#[derive(Debug)]
pub enum MirrorBookmarkValue<T> {
    Unspecified,
    Specified(T),
}

impl<T> Default for MirrorBookmarkValue<T> {
    fn default() -> Self {
        MirrorBookmarkValue::Unspecified
    }
}

impl<T> PartialEq for MirrorBookmarkValue<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &MirrorBookmarkValue<T>) -> bool {
        match (self, other) {
            (MirrorBookmarkValue::Specified(s), MirrorBookmarkValue::Specified(o)) => s == o,
            _ => true,
        }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct MirrorBookmarkItem {
    pub id: MirrorBookmarkValue<RowId>,
    pub guid: MirrorBookmarkValue<SyncGuid>,
    pub parent_guid: MirrorBookmarkValue<Option<SyncGuid>>,
    pub server_modified: MirrorBookmarkValue<ServerTimestamp>,
    pub needs_merge: MirrorBookmarkValue<bool>,
    pub validity: MirrorBookmarkValue<SyncedBookmarkValidity>,
    pub is_deleted: MirrorBookmarkValue<bool>,
    pub kind: MirrorBookmarkValue<SyncedBookmarkKind>,
    pub date_added: MirrorBookmarkValue<Timestamp>,
    pub title: MirrorBookmarkValue<Option<String>>,
    pub place_id: MirrorBookmarkValue<Option<RowId>>,
    pub keyword: MirrorBookmarkValue<Option<String>>,
    pub description: MirrorBookmarkValue<Option<String>>,
    pub load_in_sidebar: MirrorBookmarkValue<Option<bool>>,
    pub smart_bookmark_name: MirrorBookmarkValue<Option<String>>,
    pub feed_url: MirrorBookmarkValue<Option<String>>,
    pub site_url: MirrorBookmarkValue<Option<String>>,
    // Note that url is *not* in the table, but a convenience for tests.
    pub url: MirrorBookmarkValue<Option<Url>>,
}

macro_rules! impl_builder_simple {
    ($builder_name:ident, $T:ty) => {
        pub fn $builder_name<'a>(&'a mut self, val: $T) -> &'a mut MirrorBookmarkItem {
            self.$builder_name = MirrorBookmarkValue::Specified(val);
            self
        }
    };
}
macro_rules! impl_builder_ref {
    ($builder_name:ident, $T:ty) => {
        pub fn $builder_name<'a>(&'a mut self, val: &$T) -> &'a mut MirrorBookmarkItem {
            self.$builder_name = MirrorBookmarkValue::Specified((*val).clone());
            self
        }
    };
}

macro_rules! impl_builder_opt_ref {
    ($builder_name:ident, $T:ty) => {
        pub fn $builder_name<'a>(&'a mut self, val: Option<&$T>) -> &'a mut MirrorBookmarkItem {
            self.$builder_name = MirrorBookmarkValue::Specified(val.map(|v| v.clone()));
            self
        }
    };
}

macro_rules! impl_builder_opt_string {
    ($builder_name:ident) => {
        pub fn $builder_name<'a>(&'a mut self, val: Option<&str>) -> &'a mut MirrorBookmarkItem {
            self.$builder_name = MirrorBookmarkValue::Specified(val.map(|s| s.to_string()));
            self
        }
    };
}

#[allow(unused)] // not all methods here are currently used.
impl MirrorBookmarkItem {
    // A "builder" pattern, so tests can do `MirrorBookmarkItem::new().title(...).url(...)` etc
    pub fn new() -> MirrorBookmarkItem {
        MirrorBookmarkItem {
            ..Default::default()
        }
    }

    impl_builder_simple!(id, RowId);
    impl_builder_ref!(guid, SyncGuid);
    impl_builder_opt_ref!(parent_guid, SyncGuid);
    impl_builder_simple!(server_modified, ServerTimestamp);
    impl_builder_simple!(needs_merge, bool);
    impl_builder_simple!(validity, SyncedBookmarkValidity);
    impl_builder_simple!(is_deleted, bool);
    impl_builder_simple!(kind, SyncedBookmarkKind);
    impl_builder_simple!(date_added, Timestamp);
    impl_builder_opt_string!(title);

    // no place_id - we use url instead.
    pub fn url<'a>(&'a mut self, url: Option<&str>) -> &'a mut MirrorBookmarkItem {
        let url = url.map(|s| Url::parse(s).expect("should be a valid url"));
        self.url = MirrorBookmarkValue::Specified(url);
        self
    }

    impl_builder_opt_string!(keyword);
    impl_builder_opt_string!(description);
    impl_builder_simple!(load_in_sidebar, Option<bool>);
    impl_builder_opt_string!(smart_bookmark_name);
    impl_builder_opt_string!(feed_url);
    impl_builder_opt_string!(site_url);

    // Get a record from the DB.
    pub fn get(conn: &PlacesDb, guid: &SyncGuid) -> Result<Option<Self>> {
        Ok(conn.try_query_row(
            "SELECT b.*, p.url
                               FROM moz_bookmarks_synced b
                               LEFT JOIN moz_places p on b.placeId = p.id
                               WHERE b.guid = :guid",
            &[(":guid", guid)],
            Self::from_row,
            true,
        )?)
    }

    // Return a new MirrorBookmarkItem from a database row. All values will
    // be MirrorBookmarkValue::Specified.
    fn from_row(row: &Row) -> Result<Self> {
        Ok(Self {
            id: MirrorBookmarkValue::Specified(row.get_checked("id")?),
            guid: MirrorBookmarkValue::Specified(row.get_checked("guid")?),
            parent_guid: MirrorBookmarkValue::Specified(row.get_checked("parentGuid")?),
            server_modified: MirrorBookmarkValue::Specified(ServerTimestamp(
                row.get_checked::<_, f64>("serverModified")?,
            )),
            needs_merge: MirrorBookmarkValue::Specified(row.get_checked("needsMerge")?),
            validity: MirrorBookmarkValue::Specified(
                SyncedBookmarkValidity::from_u8(row.get_checked("validity")?)
                    .expect("a valid validity"),
            ),
            is_deleted: MirrorBookmarkValue::Specified(row.get_checked("isDeleted")?),
            kind: MirrorBookmarkValue::Specified(
                SyncedBookmarkKind::from_u8(row.get_checked("kind")?).expect("a valid kind"),
            ),
            date_added: MirrorBookmarkValue::Specified(row.get_checked("dateAdded")?),
            title: MirrorBookmarkValue::Specified(row.get_checked("title")?),
            place_id: MirrorBookmarkValue::Specified(row.get_checked("placeId")?),
            keyword: MirrorBookmarkValue::Specified(row.get_checked("keyword")?),
            description: MirrorBookmarkValue::Specified(row.get_checked("description")?),
            load_in_sidebar: MirrorBookmarkValue::Specified(row.get_checked("loadInSidebar")?),
            smart_bookmark_name: MirrorBookmarkValue::Specified(
                row.get_checked("smartBookmarkName")?,
            ),
            feed_url: MirrorBookmarkValue::Specified(row.get_checked("feedUrl")?),
            site_url: MirrorBookmarkValue::Specified(row.get_checked("siteUrl")?),
            url: MirrorBookmarkValue::Specified(
                row.get_checked::<_, Option<String>>("url")?
                    .and_then(|s| Url::parse(&s).ok()),
            ),
        })
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::super::bookmarks::json_tree::{self, FetchDepth};
use super::*;
use rusqlite::Row;

// A helper that will ensure tests fail, but in production will make log noise instead.
fn noisy_debug_assert_eq<T: std::cmp::PartialEq + std::fmt::Debug>(a: &T, b: &T, msg: &str) {
    debug_assert_eq!(a, b);
    if a != b {
        error_support::report_error!(
            "places-bookmarks-corruption",
            "check failed: {}: {:?} != {:?}",
            msg,
            a,
            b
        );
    }
}

fn noisy_debug_assert(v: bool, msg: &str) {
    debug_assert!(v);
    if !v {
        error_support::report_error!(
            "places-bookmark-corruption",
            "check failed: {}: expected true, got false",
            msg
        );
    }
}

/// Structs we return when reading bookmarks
#[derive(Debug, Clone)]
pub struct BookmarkData {
    pub guid: SyncGuid,
    pub parent_guid: SyncGuid,
    pub position: u32,
    pub date_added: Timestamp,
    pub last_modified: Timestamp,
    pub url: Url,
    pub title: Option<String>,
}

impl From<BookmarkData> for Item {
    fn from(b: BookmarkData) -> Self {
        Item::Bookmark { b }
    }
}

// Only for tests because we ignore timestamps
#[cfg(test)]
impl PartialEq for BookmarkData {
    fn eq(&self, other: &Self) -> bool {
        self.guid == other.guid
            && self.parent_guid == other.parent_guid
            && self.position == other.position
            && self.url == other.url
            && self.title == other.title
    }
}

#[derive(Debug, Clone)]
pub struct Separator {
    pub guid: SyncGuid,
    pub date_added: Timestamp,
    pub last_modified: Timestamp,
    pub parent_guid: SyncGuid,
    pub position: u32,
}

impl From<Separator> for Item {
    fn from(s: Separator) -> Self {
        Item::Separator { s }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Folder {
    pub guid: SyncGuid,
    pub date_added: Timestamp,
    pub last_modified: Timestamp,
    pub parent_guid: Option<SyncGuid>, // Option because the root is a folder but has no parent.
    // Always 0 if parent_guid is None
    pub position: u32,
    pub title: Option<String>,
    // Depending on the specific API request, either, both, or none of these `child_*` vecs
    // will be populated.
    pub child_guids: Option<Vec<SyncGuid>>,
    pub child_nodes: Option<Vec<Item>>,
}

impl From<Folder> for Item {
    fn from(f: Folder) -> Self {
        Item::Folder { f }
    }
}

// The type used to update the actual item.
#[derive(Debug, Clone)]
pub enum Item {
    Bookmark { b: BookmarkData },
    Separator { s: Separator },
    Folder { f: Folder },
}

// We allow all "common" fields from the sub-types to be getters on the
// InsertableItem type.
macro_rules! impl_common_bookmark_getter {
    ($getter_name:ident, $T:ty) => {
        pub fn $getter_name(&self) -> &$T {
            match self {
                Item::Bookmark { b } => &b.$getter_name,
                Item::Separator { s } => &s.$getter_name,
                Item::Folder { f } => &f.$getter_name,
            }
        }
    };
}

impl Item {
    impl_common_bookmark_getter!(guid, SyncGuid);
    impl_common_bookmark_getter!(position, u32);
    impl_common_bookmark_getter!(date_added, Timestamp);
    impl_common_bookmark_getter!(last_modified, Timestamp);
    pub fn parent_guid(&self) -> Option<&SyncGuid> {
        match self {
            Item::Bookmark { b } => Some(&b.parent_guid),
            Item::Folder { f } => f.parent_guid.as_ref(),
            Item::Separator { s } => Some(&s.parent_guid),
        }
    }
}

/// No simple `From` here, because json_tree doesn't give us the parent or position - it
/// expects us to walk a tree, so we do.
///
/// Extra complication for the fact the root has a None parent_guid :)
fn folder_from_node_with_parent_info(
    f: json_tree::FolderNode,
    parent_guid: Option<SyncGuid>,
    position: u32,
    depth_left: usize,
) -> Folder {
    let guid = f.guid.expect("all items have guids");
    // We always provide child_guids, and only provide child_nodes if we are
    // going to keep recursing.
    let child_guids = Some(
        f.children
            .iter()
            .map(|child| child.guid().clone())
            .collect(),
    );
    let child_nodes = if depth_left != 0 {
        Some(
            f.children
                .into_iter()
                .enumerate()
                .map(|(child_pos, child)| {
                    item_from_node_with_parent_info(
                        child,
                        guid.clone(),
                        child_pos as u32,
                        depth_left - 1,
                    )
                })
                .collect(),
        )
    } else {
        None
    };
    Folder {
        guid,
        parent_guid,
        position,
        child_nodes,
        child_guids,
        title: f.title,
        date_added: f.date_added.expect("always get dates"),
        last_modified: f.last_modified.expect("always get dates"),
    }
}

fn item_from_node_with_parent_info(
    n: json_tree::BookmarkTreeNode,
    parent_guid: SyncGuid,
    position: u32,
    depth_left: usize,
) -> Item {
    match n {
        json_tree::BookmarkTreeNode::Bookmark { b } => BookmarkData {
            guid: b.guid.expect("all items have guids"),
            parent_guid,
            position,
            url: b.url,
            title: b.title,
            date_added: b.date_added.expect("always get dates"),
            last_modified: b.last_modified.expect("always get dates"),
        }
        .into(),
        json_tree::BookmarkTreeNode::Separator { s } => Separator {
            guid: s.guid.expect("all items have guids"),
            parent_guid,
            position,
            date_added: s.date_added.expect("always get dates"),
            last_modified: s.last_modified.expect("always get dates"),
        }
        .into(),
        json_tree::BookmarkTreeNode::Folder { f } => {
            folder_from_node_with_parent_info(f, Some(parent_guid), position, depth_left).into()
        }
    }
}

/// Call fetch_tree_with_depth with FetchDepth::Deepest.
/// This is the function called by the FFI when requesting the tree.
pub fn fetch_tree(db: &PlacesDb, item_guid: &SyncGuid) -> Result<Option<Item>> {
    fetch_tree_with_depth(db, item_guid, &FetchDepth::Deepest)
}

/// Call fetch_tree with a depth parameter and convert the result
/// to an Item.
pub fn fetch_tree_with_depth(
    db: &PlacesDb,
    item_guid: &SyncGuid,
    target_depth: &FetchDepth,
) -> Result<Option<Item>> {
    let (tree, parent_guid, position) = if let Some((tree, parent_guid, position)) =
        json_tree::fetch_tree(db, item_guid, target_depth)?
    {
        (tree, parent_guid, position)
    } else {
        return Ok(None);
    };
    // parent_guid being an Option<> is a bit if a pain :(
    Ok(Some(match tree {
        json_tree::BookmarkTreeNode::Folder { f } => {
            noisy_debug_assert(
                parent_guid.is_none() ^ (f.guid.as_ref() != Some(BookmarkRootGuid::Root.guid())),
                "only root has no parent",
            );
            let depth_left = match target_depth {
                FetchDepth::Specific(v) => *v,
                FetchDepth::Deepest => usize::MAX,
            };
            folder_from_node_with_parent_info(f, parent_guid, position, depth_left).into()
        }
        _ => item_from_node_with_parent_info(
            tree,
            parent_guid.expect("must have parent"),
            position,
            0,
        ),
    }))
}

pub fn fetch_bookmarks_by_url(db: &PlacesDb, url: &Url) -> Result<Vec<BookmarkData>> {
    let nodes = crate::storage::bookmarks::get_raw_bookmarks_for_url(db, url)?
        .into_iter()
        .map(|rb| {
            // Cause tests to fail, but we'd rather not panic here
            // for real.
            noisy_debug_assert_eq(&rb.child_count, &0, "child count should be zero");
            noisy_debug_assert_eq(
                &rb.bookmark_type,
                &BookmarkType::Bookmark,
                "not a bookmark!",
            );
            // We don't log URLs so we do the comparison here.
            noisy_debug_assert(rb.url.as_ref() == Some(url), "urls don't match");
            noisy_debug_assert(rb.parent_guid.is_some(), "no parent guid");
            BookmarkData {
                guid: rb.guid,
                parent_guid: rb
                    .parent_guid
                    .unwrap_or_else(|| BookmarkRootGuid::Unfiled.into()),
                position: rb.position,
                date_added: rb.date_added,
                last_modified: rb.date_modified,
                url: url.clone(),
                title: rb.title,
            }
        })
        .collect::<Vec<_>>();
    Ok(nodes)
}

/// This is similar to fetch_tree, but does not recursively fetch children of
/// folders.
///
/// If `get_direct_children` is true, it will return 1 level of folder children,
/// otherwise it returns just their guids.
pub fn fetch_bookmark(
    db: &PlacesDb,
    item_guid: &SyncGuid,
    get_direct_children: bool,
) -> Result<Option<Item>> {
    let depth = if get_direct_children {
        FetchDepth::Specific(1)
    } else {
        FetchDepth::Specific(0)
    };
    fetch_tree_with_depth(db, item_guid, &depth)
}

fn bookmark_from_row(row: &Row<'_>) -> Result<Option<BookmarkData>> {
    Ok(
        match row
            .get::<_, Option<String>>("url")?
            .and_then(|href| url::Url::parse(&href).ok())
        {
            Some(url) => Some(BookmarkData {
                guid: row.get("guid")?,
                parent_guid: row.get("parentGuid")?,
                position: row.get("position")?,
                date_added: row.get("dateAdded")?,
                last_modified: row.get("lastModified")?,
                title: row.get("title")?,
                url,
            }),
            None => None,
        },
    )
}

pub fn search_bookmarks(db: &PlacesDb, search: &str, limit: u32) -> Result<Vec<BookmarkData>> {
    let scope = db.begin_interrupt_scope()?;
    Ok(db
        .query_rows_into_cached::<Vec<Option<BookmarkData>>, _, _, _, _>(
            &SEARCH_QUERY,
            &[
                (":search", &search as &dyn rusqlite::ToSql),
                (":limit", &limit),
            ],
            |row| -> Result<_> {
                scope.err_if_interrupted()?;
                bookmark_from_row(row)
            },
        )?
        .into_iter()
        .flatten()
        .collect())
}

pub fn recent_bookmarks(db: &PlacesDb, limit: u32) -> Result<Vec<BookmarkData>> {
    let scope = db.begin_interrupt_scope()?;
    Ok(db
        .query_rows_into_cached::<Vec<Option<BookmarkData>>, _, _, _, _>(
            &RECENT_BOOKMARKS_QUERY,
            &[(":limit", &limit as &dyn rusqlite::ToSql)],
            |row| -> Result<_> {
                scope.err_if_interrupted()?;
                bookmark_from_row(row)
            },
        )?
        .into_iter()
        .flatten()
        .collect())
}

lazy_static::lazy_static! {
    pub static ref SEARCH_QUERY: String = format!(
        "SELECT
            b.guid,
            p.guid AS parentGuid,
            b.position,
            b.dateAdded,
            b.lastModified,
            -- Note we return null for titles with an empty string.
            NULLIF(b.title, '') AS title,
            h.url AS url
        FROM moz_bookmarks b
        JOIN moz_bookmarks p ON p.id = b.parent
        JOIN moz_places h ON h.id = b.fk
        WHERE b.type = {bookmark_type}
            AND AUTOCOMPLETE_MATCH(
                :search, h.url, IFNULL(b.title, h.title),
                NULL, -- tags
                -- We could pass the versions of these from history in,
                -- but they're just used to figure out whether or not
                -- the query fits the given behavior, and we know
                -- we're only passing in and looking for bookmarks,
                -- so using the args from history would be pointless
                -- and would make things slower.
                0, -- visit_count
                0, -- typed
                1, -- bookmarked
                NULL, -- open page count
                {match_bhvr},
                {search_bhvr}
            )
        LIMIT :limit",
        bookmark_type = BookmarkType::Bookmark as u8,
        match_bhvr = crate::match_impl::MatchBehavior::Anywhere as u32,
        search_bhvr = crate::match_impl::SearchBehavior::BOOKMARK.bits(),
    );

    pub static ref RECENT_BOOKMARKS_QUERY: String = format!(
        "SELECT
            b.guid,
            p.guid AS parentGuid,
            b.position,
            b.dateAdded,
            b.lastModified,
            NULLIF(b.title, '') AS title,
            h.url AS url
        FROM moz_bookmarks b
        JOIN moz_bookmarks p ON p.id = b.parent
        JOIN moz_places h ON h.id = b.fk
        WHERE b.type = {bookmark_type}
        ORDER BY b.dateAdded DESC
        LIMIT :limit",
        bookmark_type = BookmarkType::Bookmark as u8
    );
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::api::places_api::test::new_mem_connections;
    use crate::tests::{append_invalid_bookmark, insert_json_tree};
    use serde_json::json;
    #[test]
    fn test_get_by_url() -> Result<()> {
        let conns = new_mem_connections();
        insert_json_tree(
            &conns.write,
            json!({
                "guid": String::from(BookmarkRootGuid::Unfiled.as_str()),
                "children": [
                    {
                        "guid": "bookmark1___",
                        "url": "https://www.example1.com/",
                        "title": "no 1",
                    },
                    {
                        "guid": "bookmark2___",
                        "url": "https://www.example2.com/a/b/c/d?q=v#abcde",
                        "title": "yes 1",
                    },
                    {
                        "guid": "bookmark3___",
                        "url": "https://www.example2.com/a/b/c/d",
                        "title": "no 2",
                    },
                    {
                        "guid": "bookmark4___",
                        "url": "https://www.example2.com/a/b/c/d?q=v#abcde",
                        "title": "yes 2",
                    },
                ]
            }),
        );
        let url = url::Url::parse("https://www.example2.com/a/b/c/d?q=v#abcde")?;
        let mut bmks = fetch_bookmarks_by_url(&conns.read, &url)?;
        bmks.sort_by_key(|b| b.guid.as_str().to_string());
        assert_eq!(bmks.len(), 2);
        assert_eq!(
            bmks[0],
            BookmarkData {
                guid: "bookmark2___".into(),
                title: Some("yes 1".into()),
                url: url.clone(),
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: 1,
                // Ignored by our PartialEq
                date_added: Timestamp(0),
                last_modified: Timestamp(0),
            }
        );
        assert_eq!(
            bmks[1],
            BookmarkData {
                guid: "bookmark4___".into(),
                title: Some("yes 2".into()),
                url,
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: 3,
                // Ignored by our PartialEq
                date_added: Timestamp(0),
                last_modified: Timestamp(0),
            }
        );

        let no_url = url::Url::parse("https://no.bookmark.com")?;
        assert!(fetch_bookmarks_by_url(&conns.read, &no_url)?.is_empty());

        Ok(())
    }
    #[test]
    fn test_search() -> Result<()> {
        let conns = new_mem_connections();
        insert_json_tree(
            &conns.write,
            json!({
                "guid": String::from(BookmarkRootGuid::Unfiled.as_str()),
                "children": [
                    {
                        "guid": "bookmark1___",
                        "url": "https://www.example1.com/",
                        "title": "",
                    },
                    {
                        "guid": "bookmark2___",
                        "url": "https://www.example2.com/a/b/c/d?q=v#example",
                        "title": "",
                    },
                    {
                        "guid": "bookmark3___",
                        "url": "https://www.example2.com/a/b/c/d",
                        "title": "",
                    },
                    {
                        "guid": "bookmark4___",
                        "url": "https://www.doesnt_match.com/a/b/c/d",
                        "title": "",
                    },
                    {
                        "guid": "bookmark5___",
                        "url": "https://www.example2.com/a/b/",
                        "title": "a b c d",
                    },
                    {
                        "guid": "bookmark6___",
                        "url": "https://www.example2.com/a/b/c/d",
                        "title": "foo bar baz",
                    },
                    {
                        "guid": "bookmark7___",
                        "url": "https://www.1234.com/a/b/c/d",
                        "title": "my example bookmark",
                    },
                ]
            }),
        );
        append_invalid_bookmark(
            &conns.write,
            BookmarkRootGuid::Unfiled.guid(),
            "invalid",
            "badurl",
        );
        let mut bmks = search_bookmarks(&conns.read, "ample", 10)?;
        bmks.sort_by_key(|b| b.guid.as_str().to_string());
        assert_eq!(bmks.len(), 6);
        let expect = [
            ("bookmark1___", "https://www.example1.com/", "", 0),
            (
                "bookmark2___",
                "https://www.example2.com/a/b/c/d?q=v#example",
                "",
                1,
            ),
            ("bookmark3___", "https://www.example2.com/a/b/c/d", "", 2),
            (
                "bookmark5___",
                "https://www.example2.com/a/b/",
                "a b c d",
                4,
            ),
            (
                "bookmark6___",
                "https://www.example2.com/a/b/c/d",
                "foo bar baz",
                5,
            ),
            (
                "bookmark7___",
                "https://www.1234.com/a/b/c/d",
                "my example bookmark",
                6,
            ),
        ];
        for (got, want) in bmks.iter().zip(expect.iter()) {
            assert_eq!(got.guid.as_str(), want.0);
            assert_eq!(got.url, url::Url::parse(want.1).unwrap());
            assert_eq!(got.title.as_ref().unwrap_or(&String::new()), want.2);
            assert_eq!(got.position, want.3);
            assert_eq!(got.parent_guid, BookmarkRootGuid::Unfiled);
        }
        Ok(())
    }
    #[test]
    fn test_fetch_bookmark() -> Result<()> {
        let conns = new_mem_connections();

        insert_json_tree(
            &conns.write,
            json!({
                "guid": BookmarkRootGuid::Mobile.as_guid(),
                "children": [
                    {
                        "guid": "bookmark1___",
                        "url": "https://www.example1.com/"
                    },
                    {
                        "guid": "bookmark2___",
                        "url": "https://www.example2.com/"
                    },
                ]
            }),
        );

        // Put a couple of invalid items in the tree - not only should fetching
        // them directly "work" (as in, not crash!), fetching their parent's
        // tree should also do a sane thing (ie, not crash *and* return the
        // valid items)
        let guid_bad = append_invalid_bookmark(
            &conns.write,
            BookmarkRootGuid::Mobile.guid(),
            "invalid url",
            "badurl",
        )
        .guid;
        assert!(fetch_bookmark(&conns.read, &guid_bad, false)?.is_none());

        // Now fetch the entire tree.
        let root = match fetch_bookmark(&conns.read, BookmarkRootGuid::Root.guid(), false)?.unwrap()
        {
            Item::Folder { f } => f,
            _ => panic!("root not a folder?"),
        };
        assert!(root.child_guids.is_some());
        assert!(root.child_nodes.is_none());
        assert_eq!(root.child_guids.unwrap().len(), 4);

        let root = match fetch_bookmark(&conns.read, BookmarkRootGuid::Root.guid(), true)?.unwrap()
        {
            Item::Folder { f } => f,
            _ => panic!("not a folder?"),
        };

        assert!(root.child_nodes.is_some());
        assert!(root.child_guids.is_some());
        assert_eq!(
            root.child_guids.unwrap(),
            root.child_nodes
                .as_ref()
                .unwrap()
                .iter()
                .map(|c| c.guid().clone())
                .collect::<Vec<SyncGuid>>()
        );
        let root_children = root.child_nodes.unwrap();
        assert_eq!(root_children.len(), 4);
        for child in root_children {
            match child {
                Item::Folder { f: child } => {
                    assert!(child.child_guids.is_some());
                    assert!(child.child_nodes.is_none());
                    if child.guid == BookmarkRootGuid::Mobile {
                        assert_eq!(
                            child.child_guids.unwrap(),
                            &[
                                SyncGuid::from("bookmark1___"),
                                SyncGuid::from("bookmark2___")
                            ]
                        );
                    }
                }
                _ => panic!("all root children should be folders"),
            }
        }

        let unfiled =
            match fetch_bookmark(&conns.read, BookmarkRootGuid::Unfiled.guid(), false)?.unwrap() {
                Item::Folder { f } => f,
                _ => panic!("not a folder?"),
            };

        assert!(unfiled.child_guids.is_some());
        assert!(unfiled.child_nodes.is_none());
        assert_eq!(unfiled.child_guids.unwrap().len(), 0);

        let unfiled =
            match fetch_bookmark(&conns.read, BookmarkRootGuid::Unfiled.guid(), true)?.unwrap() {
                Item::Folder { f } => f,
                _ => panic!("not a folder?"),
            };
        assert!(unfiled.child_guids.is_some());
        assert!(unfiled.child_nodes.is_some());

        assert_eq!(unfiled.child_nodes.unwrap().len(), 0);
        assert_eq!(unfiled.child_guids.unwrap().len(), 0);

        assert!(fetch_bookmark(&conns.read, &"not_exist___".into(), true)?.is_none());
        Ok(())
    }
    #[test]
    fn test_fetch_tree() -> Result<()> {
        let conns = new_mem_connections();

        insert_json_tree(
            &conns.write,
            json!({
                "guid": BookmarkRootGuid::Mobile.as_guid(),
                "children": [
                    {
                        "guid": "bookmark1___",
                        "url": "https://www.example1.com/"
                    },
                    {
                        "guid": "bookmark2___",
                        "url": "https://www.example2.com/"
                    },
                ]
            }),
        );

        append_invalid_bookmark(
            &conns.write,
            BookmarkRootGuid::Mobile.guid(),
            "invalid url",
            "badurl",
        );

        let root = match fetch_tree(&conns.read, BookmarkRootGuid::Root.guid())?.unwrap() {
            Item::Folder { f } => f,
            _ => panic!("root not a folder?"),
        };
        assert!(root.parent_guid.is_none());
        assert_eq!(root.position, 0);

        assert!(root.child_guids.is_some());
        let children = root.child_nodes.as_ref().unwrap();
        assert_eq!(
            root.child_guids.unwrap(),
            children
                .iter()
                .map(|c| c.guid().clone())
                .collect::<Vec<SyncGuid>>()
        );
        let mut mobile_pos = None;
        for (i, c) in children.iter().enumerate() {
            assert_eq!(i as u32, *c.position());
            assert_eq!(c.parent_guid().unwrap(), &root.guid);
            match c {
                Item::Folder { f } => {
                    // all out roots are here, so check it is mobile.
                    if f.guid == BookmarkRootGuid::Mobile {
                        assert!(f.child_guids.is_some());
                        assert!(f.child_nodes.is_some());
                        let child_nodes = f.child_nodes.as_ref().unwrap();
                        assert_eq!(
                            f.child_guids.as_ref().unwrap(),
                            &child_nodes
                                .iter()
                                .map(|c| c.guid().clone())
                                .collect::<Vec<SyncGuid>>()
                        );
                        mobile_pos = Some(i as u32);
                        let b = match &child_nodes[0] {
                            Item::Bookmark { b } => b,
                            _ => panic!("expect a bookmark"),
                        };
                        assert_eq!(b.position, 0);
                        assert_eq!(b.guid, SyncGuid::from("bookmark1___"));
                        assert_eq!(b.url, Url::parse("https://www.example1.com/").unwrap());

                        let b = match &child_nodes[1] {
                            Item::Bookmark { b } => b,
                            _ => panic!("expect a bookmark"),
                        };
                        assert_eq!(b.position, 1);
                        assert_eq!(b.guid, SyncGuid::from("bookmark2___"));
                        assert_eq!(b.url, Url::parse("https://www.example2.com/").unwrap());
                    }
                }
                _ => panic!("unexpected type"),
            }
        }
        // parent_guid/position for the directly returned node is filled in separately,
        // so make sure it works for non-root nodes too.
        let mobile = match fetch_tree(&conns.read, BookmarkRootGuid::Mobile.guid())?.unwrap() {
            Item::Folder { f } => f,
            _ => panic!("not a folder?"),
        };
        assert_eq!(mobile.parent_guid.unwrap(), BookmarkRootGuid::Root);
        assert_eq!(mobile.position, mobile_pos.unwrap());

        let bm1 = match fetch_tree(&conns.read, &SyncGuid::from("bookmark1___"))?.unwrap() {
            Item::Bookmark { b } => b,
            _ => panic!("not a bookmark?"),
        };
        assert_eq!(bm1.parent_guid, BookmarkRootGuid::Mobile);
        assert_eq!(bm1.position, 0);

        Ok(())
    }
    #[test]
    fn test_recent() -> Result<()> {
        let conns = new_mem_connections();
        let kids = [
            json!({
                "guid": "bookmark1___",
                "url": "https://www.example1.com/",
                "title": "b1",
            }),
            json!({
                "guid": "bookmark2___",
                "url": "https://www.example2.com/",
                "title": "b2",
            }),
            json!({
                "guid": "bookmark3___",
                "url": "https://www.example3.com/",
                "title": "b3",
            }),
            json!({
                "guid": "bookmark4___",
                "url": "https://www.example4.com/",
                "title": "b4",
            }),
            // should be ignored.
            json!({
                "guid": "folder1_____",
                "title": "A folder",
                "children": []
            }),
            json!({
                "guid": "bookmark5___",
                "url": "https://www.example5.com/",
                "title": "b5",
            }),
        ];
        for k in &kids {
            insert_json_tree(
                &conns.write,
                json!({
                    "guid": String::from(BookmarkRootGuid::Unfiled.as_str()),
                    "children": [k.clone()],
                }),
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        append_invalid_bookmark(
            &conns.write,
            BookmarkRootGuid::Unfiled.guid(),
            "invalid url",
            "badurl",
        );

        // The limit applies before we filter the invalid bookmark, so ask for 4.
        let bmks = recent_bookmarks(&conns.read, 4)?;
        assert_eq!(bmks.len(), 3);

        assert_eq!(
            bmks[0],
            BookmarkData {
                guid: "bookmark5___".into(),
                title: Some("b5".into()),
                url: Url::parse("https://www.example5.com/").unwrap(),
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: 5,
                // Ignored by our PartialEq
                date_added: Timestamp(0),
                last_modified: Timestamp(0),
            }
        );
        assert_eq!(
            bmks[1],
            BookmarkData {
                guid: "bookmark4___".into(),
                title: Some("b4".into()),
                url: Url::parse("https://www.example4.com/").unwrap(),
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: 3,
                // Ignored by our PartialEq
                date_added: Timestamp(0),
                last_modified: Timestamp(0),
            }
        );
        assert_eq!(
            bmks[2],
            BookmarkData {
                guid: "bookmark3___".into(),
                title: Some("b3".into()),
                url: Url::parse("https://www.example3.com/").unwrap(),
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: 2,
                // Ignored by our PartialEq
                date_added: Timestamp(0),
                last_modified: Timestamp(0),
            }
        );
        Ok(())
    }
}

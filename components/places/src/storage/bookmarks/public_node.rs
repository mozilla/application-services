/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::*;
use crate::msg_types::BookmarkNode as ProtoBookmark;
use sql_support::SqlInterruptScope;

/// This type basically exists to become a msg_types::BookmarkNode, but is
/// slightly less of a pain to deal with in rust.
#[derive(Debug, Clone)]
pub struct PublicNode {
    pub node_type: BookmarkType,
    pub guid: SyncGuid,
    pub parent_guid: Option<SyncGuid>,
    // Always 0 if parent_guid is None
    pub position: u32,
    pub date_added: Timestamp,
    pub last_modified: Timestamp,
    pub url: Option<Url>,
    pub title: Option<String>,
    pub child_guids: Option<Vec<SyncGuid>>,
    pub child_nodes: Option<Vec<PublicNode>>,
}

impl Default for PublicNode {
    fn default() -> Self {
        Self {
            // Note: we mainly want `Default::default()` for filling in the
            // missing part of struct decls.
            node_type: BookmarkType::Separator,
            guid: SyncGuid(String::default()),
            parent_guid: None,
            position: 0,
            date_added: Timestamp(0),
            last_modified: Timestamp(0),
            url: None,
            title: None,
            child_guids: None,
            child_nodes: None,
        }
    }
}

impl PartialEq for PublicNode {
    fn eq(&self, other: &PublicNode) -> bool {
        // Compare everything except date_added and last_modified.
        self.node_type == other.node_type
            && self.guid == other.guid
            && self.parent_guid == other.parent_guid
            && self.url == other.url
            && self.child_guids == other.child_guids
            && self.child_nodes == other.child_nodes
    }
}

pub fn fetch_bookmarks_by_url(db: &PlacesDb, url: &Url) -> Result<Vec<PublicNode>> {
    let nodes = get_raw_bookmarks_for_url(db, url)?
        .into_iter()
        .map(|rb| {
            // Cause tests to fail, but we'd rather not panic here
            // for real.
            debug_assert_eq!(rb.child_count, 0);
            debug_assert_eq!(rb.bookmark_type, BookmarkType::Bookmark);
            debug_assert_eq!(rb.url.as_ref(), Some(url));
            PublicNode {
                node_type: rb.bookmark_type,
                guid: rb.guid,
                parent_guid: rb.parent_guid,
                position: rb.position,
                date_added: rb.date_added,
                last_modified: rb.date_modified,
                url: rb.url,
                title: rb.title,
                child_guids: None,
                child_nodes: None,
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
///
/// It also produces the protobuf message type directly, rather than
/// add a special variant of this bookmark type just for this function.
pub fn fetch_bookmark(
    db: &PlacesDb,
    item_guid: &SyncGuid,
    get_direct_children: bool,
) -> Result<Option<PublicNode>> {
    let _tx = db.begin_transaction()?;
    let scope = db.begin_interrupt_scope();
    let bookmark = fetch_bookmark_in_tx(db, item_guid, get_direct_children, &scope)?;
    // Note: We let _tx drop (which means it does a rollback) since it doesn't
    // matter, we just are using a transaction to ensure things don't change out
    // from under us, since this executes more than one query.
    Ok(bookmark)
}

fn get_child_guids(db: &PlacesDb, parent: RowId) -> Result<Vec<SyncGuid>> {
    Ok(db.query_rows_into(
        "SELECT guid FROM moz_bookmarks
         WHERE parent = :parent
         ORDER BY position ASC",
        &[(":parent", &parent)],
        |row| row.get(0),
    )?)
}

enum ChildInfo {
    NoChildren,
    Guids(Vec<SyncGuid>),
    Nodes(Vec<PublicNode>),
}

impl ChildInfo {
    fn guids_nodes(self) -> (Option<Vec<SyncGuid>>, Option<Vec<PublicNode>>) {
        match self {
            ChildInfo::NoChildren => (None, None),
            ChildInfo::Guids(g) => (Some(g), None),
            ChildInfo::Nodes(n) => (None, Some(n)),
        }
    }
}

fn fetch_bookmark_child_info(
    db: &PlacesDb,
    parent: &RawBookmark,
    get_direct_children: bool,
    scope: &SqlInterruptScope,
) -> Result<ChildInfo> {
    if parent.bookmark_type != BookmarkType::Folder {
        return Ok(ChildInfo::NoChildren);
    }
    // If we already know that we have no children, don't
    // bother querying for them.
    if parent.child_count == 0 {
        return Ok(if get_direct_children {
            ChildInfo::Nodes(vec![])
        } else {
            ChildInfo::Guids(vec![])
        });
    }
    if !get_direct_children {
        // Just get the guids.
        return Ok(ChildInfo::Guids(get_child_guids(db, parent.row_id)?));
    }
    // Fetch children. the future this should probably be done by allowing a
    // depth parameter to be passed into fetch_tree.
    let child_nodes = get_raw_bookmarks_with_parent(db, parent.row_id)?
        .into_iter()
        .map(|kid| {
            let child_guids = if kid.bookmark_type == BookmarkType::Folder {
                if kid.child_count == 0 {
                    Some(vec![])
                } else {
                    Some(get_child_guids(db, kid.row_id)?)
                }
            } else {
                None
            };
            scope.err_if_interrupted()?;
            Ok(PublicNode::from(kid).with_children(child_guids, None))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ChildInfo::Nodes(child_nodes))
}

// Implementation of fetch_bookmark
fn fetch_bookmark_in_tx(
    db: &PlacesDb,
    item_guid: &SyncGuid,
    get_direct_children: bool,
    scope: &SqlInterruptScope,
) -> Result<Option<PublicNode>> {
    let rb = if let Some(raw) = get_raw_bookmark(db, item_guid)? {
        raw
    } else {
        return Ok(None);
    };

    scope.err_if_interrupted()?;
    // If we're a folder that has children, fetch child guids or children.
    let (child_guids, child_nodes) =
        fetch_bookmark_child_info(db, &rb, get_direct_children, scope)?.guids_nodes();

    Ok(Some(
        PublicNode::from(rb).with_children(child_guids, child_nodes),
    ))
}

pub fn update_bookmark_from_message(db: &PlacesDb, msg: ProtoBookmark) -> Result<()> {
    let info = conversions::BookmarkUpdateInfo::from(msg);

    let tx = db.begin_transaction()?;
    let node_type: BookmarkType = db.query_row_and_then_named(
        "SELECT type FROM moz_bookmarks WHERE guid = :guid",
        &[(":guid", &info.guid)],
        |r| r.get(0),
        true,
    )?;
    let (guid, updatable) = info.into_updatable(node_type)?;

    update_bookmark_in_tx(db, &guid, &updatable)?;
    tx.commit()?;
    Ok(())
}

/// Call fetch_tree, convert the result to a ProtoBookmark, and ensure the
/// requested item's position and parent info are provided as well. This is the
/// function called by the FFI when requesting the tree.
pub fn fetch_public_tree(db: &PlacesDb, item_guid: &SyncGuid) -> Result<Option<PublicNode>> {
    let _tx = db.begin_transaction()?;
    let tree = if let Some(tree) = fetch_tree(db, item_guid)? {
        tree
    } else {
        return Ok(None);
    };

    // `position` and `parent_guid` will be handled for the children of
    // `item_guid` by `PublicNode::from` automatically, however we
    // still need to fill in it's own `parent_guid` and `position`.
    let mut proto = PublicNode::from(tree);

    if item_guid != BookmarkRootGuid::Root {
        let sql = "
            SELECT
                p.guid AS parent_guid,
                b.position AS position
            FROM moz_bookmarks b
            LEFT JOIN moz_bookmarks p ON p.id = b.parent
            WHERE b.guid = :guid
        ";
        let (parent_guid, position) = db.query_row_and_then_named(
            sql,
            &[(":guid", &item_guid)],
            |row| -> Result<_> { Ok((row.get::<_, Option<SyncGuid>>(0)?, row.get::<_, u32>(1)?)) },
            true,
        )?;
        proto.parent_guid = parent_guid;
        proto.position = position;
    }
    Ok(Some(proto))
}

pub fn search_bookmarks(db: &PlacesDb, search: &str, limit: u32) -> Result<Vec<PublicNode>> {
    let scope = db.begin_interrupt_scope();
    Ok(db.query_rows_into_cached(
        &SEARCH_QUERY,
        &[(":search", &search), (":limit", &limit)],
        |row| -> Result<_> {
            scope.err_if_interrupted()?;
            Ok(PublicNode {
                node_type: BookmarkType::Bookmark,
                guid: row.get("guid")?,
                parent_guid: row.get("parentGuid")?,
                position: row.get("position")?,
                date_added: row.get("dateAdded")?,
                last_modified: row.get("lastModified")?,
                title: row.get("title")?,
                url: row
                    .get::<_, Option<String>>("url")?
                    .map(|href| url::Url::parse(&href))
                    .transpose()?,
                child_guids: None,
                child_nodes: None,
            })
        },
    )?)
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
}
#[cfg(test)]
mod test {
    use super::*;
    use crate::api::places_api::test::new_mem_connection;
    use crate::tests::insert_json_tree;
    use serde_json::json;
    #[test]
    fn test_get_by_url() -> Result<()> {
        let conn = new_mem_connection();
        let _ = env_logger::try_init();
        insert_json_tree(
            &conn,
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
        let mut bmks = fetch_bookmarks_by_url(&conn, &url)?;
        bmks.sort_by_key(|b| b.guid.0.clone());
        assert_eq!(bmks.len(), 2);
        assert_eq!(
            bmks[0],
            PublicNode {
                node_type: BookmarkType::Bookmark,
                guid: "bookmark2___".into(),
                title: Some("yes 1".into()),
                url: Some(url.clone()),
                parent_guid: Some(BookmarkRootGuid::Unfiled.into()),
                position: 1,
                child_guids: None,
                child_nodes: None,
                // Ignored by our PartialEq
                date_added: Timestamp(0),
                last_modified: Timestamp(0),
            }
        );
        assert_eq!(
            bmks[1],
            PublicNode {
                node_type: BookmarkType::Bookmark,
                guid: "bookmark4___".into(),
                title: Some("yes 2".into()),
                url: Some(url.clone()),
                parent_guid: Some(BookmarkRootGuid::Unfiled.into()),
                position: 3,
                child_guids: None,
                child_nodes: None,
                // Ignored by our PartialEq
                date_added: Timestamp(0),
                last_modified: Timestamp(0),
            }
        );

        Ok(())
    }
    #[test]
    fn test_search() -> Result<()> {
        let conn = new_mem_connection();
        let _ = env_logger::try_init();
        insert_json_tree(
            &conn,
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
        let mut bmks = search_bookmarks(&conn, "ample", 10)?;
        bmks.sort_by_key(|b| b.guid.0.clone());
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
            assert_eq!(&got.guid.0, want.0);
            assert_eq!(got.url.as_ref().unwrap(), &url::Url::parse(want.1).unwrap());
            assert_eq!(got.title.as_ref().unwrap_or(&String::new()), want.2);
            assert_eq!(got.position, want.3);
            assert_eq!(got.parent_guid.as_ref().unwrap(), BookmarkRootGuid::Unfiled);
            assert_eq!(got.node_type, BookmarkType::Bookmark);
            assert!(got.child_guids.is_none());
            assert!(got.child_nodes.is_none());
        }
        Ok(())
    }
    #[test]
    fn test_fetch_bookmark() -> Result<()> {
        let conn = new_mem_connection();
        let _ = env_logger::try_init();

        insert_json_tree(
            &conn,
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

        let root = fetch_bookmark(&conn, BookmarkRootGuid::Root.guid(), false)?.unwrap();

        assert!(root.child_guids.is_some());
        assert!(root.child_nodes.is_none());
        assert_eq!(root.child_guids.unwrap().len(), 4);

        let root = fetch_bookmark(&conn, BookmarkRootGuid::Root.guid(), true)?.unwrap();

        assert!(root.child_guids.is_none());
        assert!(root.child_nodes.is_some());
        let root_children = root.child_nodes.unwrap();
        assert_eq!(root_children.len(), 4);
        for child in root_children {
            assert!(child.child_guids.is_some());
            assert!(child.child_nodes.is_none());
            if child.guid == BookmarkRootGuid::Mobile {
                assert_eq!(
                    child.child_guids.unwrap(),
                    &[
                        SyncGuid("bookmark1___".into()),
                        SyncGuid("bookmark2___".into())
                    ]
                );
            } else {
                assert_eq!(child.child_guids.unwrap().len(), 0);
            }
        }

        let unfiled = fetch_bookmark(&conn, BookmarkRootGuid::Unfiled.guid(), false)?.unwrap();
        assert!(unfiled.child_guids.is_some());
        assert!(unfiled.child_nodes.is_none());
        assert_eq!(unfiled.child_guids.unwrap().len(), 0);

        let unfiled = fetch_bookmark(&conn, BookmarkRootGuid::Unfiled.guid(), true)?.unwrap();
        assert!(unfiled.child_guids.is_none());
        assert!(unfiled.child_nodes.is_some());
        assert_eq!(unfiled.child_nodes.unwrap().len(), 0);
        Ok(())
    }
    #[test]
    fn test_fetch_tree() -> Result<()> {
        let conn = new_mem_connection();
        let _ = env_logger::try_init();

        insert_json_tree(
            &conn,
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

        let root = fetch_public_tree(&conn, BookmarkRootGuid::Root.guid())?.unwrap();
        assert!(root.parent_guid.is_none());
        assert_eq!(root.position, 0);

        assert!(root.child_guids.is_none());
        let children = root.child_nodes.as_ref().unwrap();
        let mut mobile_pos = None;
        for (i, c) in children.iter().enumerate() {
            assert_eq!(i as u32, c.position);
            assert_eq!(c.parent_guid.as_ref().unwrap(), &root.guid);
            assert!(c.child_guids.is_none());
            if c.guid == BookmarkRootGuid::Mobile {
                mobile_pos = Some(c.position);
            }
            for (i2, c2) in c.child_nodes.as_ref().unwrap().iter().enumerate() {
                assert_eq!(i2 as u32, c2.position);
                assert_eq!(c2.parent_guid.as_ref().unwrap(), &c.guid);
            }
        }
        // parent_guid/position for the directly returned node is filled in separately,
        // so make sure it works for non-root nodes too.
        let mobile = fetch_public_tree(&conn, BookmarkRootGuid::Mobile.guid())?.unwrap();
        assert_eq!(mobile.parent_guid.unwrap(), BookmarkRootGuid::Root);
        assert_eq!(mobile.position, mobile_pos.unwrap());

        Ok(())
    }
}

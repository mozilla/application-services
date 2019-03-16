/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::*;
use crate::msg_types::BookmarkNode as ProtoBookmark;

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
    Ok(nodes.into())
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
    let _tx = db.unchecked_transaction()?;
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
        |row| row.get_checked(0),
    )?)
}

fn fetch_bookmark_child_info(
    db: &PlacesDb,
    parent: &RawBookmark,
    get_direct_children: bool,
    scope: &crate::db::InterruptScope,
) -> Result<(Option<Vec<SyncGuid>>, Option<Vec<PublicNode>>)> {
    if parent.bookmark_type != BookmarkType::Folder {
        return Ok((None, None));
    }
    // If we already know that we have no children, don't
    // bother querying for them.
    if parent.child_count == 0 {
        return Ok(if get_direct_children {
            (None, Some(vec![]))
        } else {
            (Some(vec![]), None)
        });
    }
    if !get_direct_children {
        // Just get the guids.
        return Ok((Some(get_child_guids(db, parent.row_id)?), None));
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
    Ok((None, Some(child_nodes)))
}

// Implementation of fetch_bookmark
fn fetch_bookmark_in_tx(
    db: &PlacesDb,
    item_guid: &SyncGuid,
    get_direct_children: bool,
    scope: &crate::db::InterruptScope,
) -> Result<Option<PublicNode>> {
    // get_raw_bookmark doesn't work for the bookmark root, so we just return None explicitly
    // (rather than erroring). This isn't ideal, but there's no point to fetching the "true"
    // bookmark root without fetching it's children too, so whatever.
    let rb = if let Some(raw) = get_raw_bookmark(db, item_guid)? {
        raw
    } else {
        return Ok(None);
    };

    scope.err_if_interrupted()?;
    // If we're a folder that has children, fetch child guids or children.
    let (child_guids, child_nodes) =
        fetch_bookmark_child_info(db, &rb, get_direct_children, scope)?;

    Ok(Some(
        PublicNode::from(rb).with_children(child_guids, child_nodes),
    ))
}

pub fn update_bookmark_from_message(db: &PlacesDb, msg: ProtoBookmark) -> Result<()> {
    let info = conversions::BookmarkUpdateInfo::from(msg);

    let tx = db.unchecked_transaction()?;
    let node_type: BookmarkType = db.query_row_and_then_named(
        "SELECT type FROM moz_bookmarks WHERE guid = :guid",
        &[(":guid", &info.guid)],
        |r| r.get_checked(0),
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
    let _tx = db.unchecked_transaction()?;
    let tree = if let Some(tree) = fetch_tree(db, item_guid)? {
        tree
    } else {
        return Ok(None);
    };

    // `position` and `parent_guid` will be handled for the children of
    // `item_guid` by `PublicNode::from` automatically, however we
    // still need to fill in it's own `parent_guid` and `position`.
    let mut proto = PublicNode::from(tree);

    if item_guid != &BookmarkRootGuid::Root {
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
            |row| -> Result<_> {
                Ok((
                    row.get_checked::<_, Option<SyncGuid>>(0)?,
                    row.get_checked::<_, u32>(1)?,
                ))
            },
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
                guid: row.get_checked("guid")?,
                parent_guid: row.get_checked("parentGuid")?,
                position: row.get_checked("position")?,
                date_added: row.get_checked("dateAdded")?,
                last_modified: row.get_checked("lastModified")?,
                title: row.get_checked("title")?,
                url: row
                    .get_checked::<_, Option<String>>("url")?
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
                :search, h.url, IFNULL(title, h.title),
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

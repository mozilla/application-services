/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This file is for functions that return protobuf generated structs directly,
//! for FFI purposes.
//!
//! Some of these should possibly turn into functions that return a rust type,
//! which we should then convert, but currently none of the rust types is a good
//! match for what we send over the FFI (and the protobuf message is, of course,
//! since it's actually the thing we expose).
//!
//! It doesn't seem valuable to create a type that exists just to hide the use of
//! types from `msg_types`, so we just do this.

use super::*;
use crate::msg_types::{BookmarkNode as ProtoBookmark, BookmarkNodeList as ProtoNodeList};

pub fn fetch_bookmarks_by_url(db: &PlacesDb, url: &Url) -> Result<ProtoNodeList> {
    let nodes = get_raw_bookmarks_for_url(db, url)?
        .into_iter()
        .map(|rb| {
            // Cause tests to fail, but we'd rather not panic here
            // for real.
            debug_assert_eq!(rb.child_count, 0);
            debug_assert_eq!(rb.bookmark_type, BookmarkType::Bookmark);
            debug_assert!(rb.url.is_some());
            ProtoBookmark {
                node_type: Some(rb.bookmark_type as i32),
                guid: Some(rb.guid.0),
                parent_guid: Some(rb.parent_guid.0),
                position: Some(rb.position),
                date_added: Some(rb.date_added.0 as i64),
                last_modified: Some(rb.date_modified.0 as i64),
                url: rb.url.map(|u| u.into_string()),
                title: rb.title,
                child_guids: vec![],
                child_nodes: vec![],
                have_child_nodes: None,
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
) -> Result<Option<ProtoBookmark>> {
    let _tx = db.unchecked_transaction()?;
    let bookmark = fetch_bookmark_in_tx(db, item_guid, get_direct_children)?;
    // Note: We let _tx drop (which means it does a rollback) since it doesn't
    // matter, we just are using a transaction to ensure things don't change out
    // from under us, since this executes more than one query.
    Ok(bookmark)
}

// Implementation of fetch_bookmark
fn fetch_bookmark_in_tx(
    db: &PlacesDb,
    item_guid: &SyncGuid,
    get_direct_children: bool,
) -> Result<Option<ProtoBookmark>> {
    // get_raw_bookmark doesn't work for the bookmark root, so we just return None explicitly
    // (rather than erroring). This isn't ideal, but there's no point to fetching the "true"
    // bookmark root without fetching it's children too, so whatever.
    if item_guid == &BookmarkRootGuid::Root {
        return Ok(None);
    }

    let rb = if let Some(raw) = get_raw_bookmark(db, item_guid)? {
        raw
    } else {
        return Ok(None);
    };

    // If we're a folder that has children, fetch child guids or children depending.
    let (child_guids, child_nodes) =
        if rb.bookmark_type == BookmarkType::Folder && rb.child_count != 0 {
            let child_guids: Vec<String> = db.query_rows_into(
                "SELECT guid
                 FROM moz_bookmarks
                 WHERE parent = :parent
                 ORDER BY position ASC",
                &[(":parent", &rb.row_id)],
                |row| row.get_checked(0),
            )?;
            if get_direct_children {
                let children: Vec<_> = child_guids
                    .into_iter()
                    .map(|guid_string| {
                        let child_guid = SyncGuid(guid_string);
                        if let Some(bmk) = fetch_bookmark_in_tx(db, &child_guid, false)? {
                            Ok(bmk)
                        } else {
                            // Not ideal (since this shouldn't be possible, we're in
                            // a transaciton, and just fetched these guids), but
                            // restructuring our queries so that this is impossible
                            // is tricky, and it seems better to have an error
                            // that's never actually used than to unwrap()
                            Err(Error::from(Corruption::MissingChild {
                                parent: item_guid.0.clone(),
                                child: child_guid.0,
                            }))
                        }
                    })
                    .collect::<Result<_>>()?;
                // Note: even though we have the child guids, we don't return them
                // because we don't want to send both over the FFI, and the child nodes
                // should have enough information.
                (vec![], children)
            } else {
                (child_guids, vec![])
            }
        } else {
            (vec![], vec![])
        };

    let result = ProtoBookmark {
        node_type: Some(rb.bookmark_type as i32),
        guid: Some(rb.guid.0),
        parent_guid: Some(rb.parent_guid.0),
        position: Some(rb.position),
        date_added: Some(rb.date_added.0 as i64),
        last_modified: Some(rb.date_modified.0 as i64),
        url: rb.url.map(|u| u.into_string()),
        title: rb.title,
        child_guids,
        child_nodes,
        have_child_nodes: Some(rb.bookmark_type == BookmarkType::Folder && get_direct_children),
    };

    Ok(Some(result))
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
/// requested item's position and parent info are provided as well. This is
/// the function called by the FFI when requesting the tree.
pub fn fetch_proto_tree(db: &PlacesDb, item_guid: &SyncGuid) -> Result<Option<ProtoBookmark>> {
    let _tx = db.unchecked_transaction()?;
    let tree = if let Some(tree) = fetch_tree(db, item_guid)? {
        tree
    } else {
        return Ok(None);
    };

    // `position` and `parent_guid` will be handled for the children of
    // `item_guid` by `ProtoBookmark::from` automatically, however we
    // still need to fill in it's own `parent_guid` and `position`.
    let mut proto = ProtoBookmark::from(tree);

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
                    row.get_checked::<_, String>(0)?,
                    row.get_checked::<_, u32>(1)?,
                ))
            },
            true,
        )?;
        proto.parent_guid = Some(parent_guid);
        proto.position = Some(position);
    }
    Ok(Some(proto))
}

pub fn search_bookmarks(db: &impl ConnExt, search: &str, limit: u32) -> Result<ProtoNodeList> {
    let nodes: Vec<_> = db.query_rows_into_cached(
        &SEARCH_QUERY,
        &[(":search", &search), (":limit", &limit)],
        |row| -> Result<_> {
            Ok(ProtoBookmark {
                node_type: Some(BookmarkType::Bookmark as i32),
                guid: Some(row.get_checked("guid")?),
                parent_guid: Some(row.get_checked("parentGuid")?),
                position: Some(row.get_checked("position")?),
                date_added: Some(row.get_checked("dateAdded")?),
                last_modified: Some(row.get_checked("lastModified")?),
                title: row.get_checked("title")?,
                url: Some(row.get_checked("url")?),
                child_guids: vec![],
                child_nodes: vec![],
                have_child_nodes: None,
            })
        },
    )?;
    Ok(nodes.into())
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

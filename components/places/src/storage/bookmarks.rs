/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::RowId;
use super::{fetch_page_info, new_page_info};
use crate::db::PlacesDb;
use crate::error::*;
use crate::types::{BookmarkType, SyncGuid, SyncStatus, Timestamp};
use rusqlite::types::ToSql;
use rusqlite::{Connection, Row};
use serde::{
    de::{Deserialize, Deserializer},
    ser::{Serialize, SerializeStruct, Serializer},
};
use serde_derive::*;
#[cfg(test)]
use serde_json::{self, json};
use sql_support::{self, ConnExt};
use std::cmp::{max, min};
use std::collections::HashMap;
use url::Url;

pub use public_node::PublicNode;
pub use root_guid::BookmarkRootGuid;

mod conversions;
pub mod public_node;
mod root_guid;

fn create_root(
    db: &Connection,
    title: &str,
    guid: &SyncGuid,
    position: u32,
    when: Timestamp,
) -> Result<()> {
    let sql = format!(
        "
        INSERT INTO moz_bookmarks
            (type, position, title, dateAdded, lastModified, guid, parent,
             syncChangeCounter, syncStatus)
        VALUES
            (:item_type, :item_position, :item_title, :date_added, :last_modified, :guid,
             (SELECT id FROM moz_bookmarks WHERE guid = {:?}),
             1, :sync_status)
        ",
        BookmarkRootGuid::Root.as_guid().0
    );
    let params: Vec<(&str, &ToSql)> = vec![
        (":item_type", &BookmarkType::Folder),
        (":item_position", &position),
        (":item_title", &title),
        (":date_added", &when),
        (":last_modified", &when),
        (":guid", guid),
        (":sync_status", &SyncStatus::New),
    ];
    db.execute_named_cached(&sql, &params)?;
    Ok(())
}

pub fn create_bookmark_roots(db: &Connection) -> Result<()> {
    let now = Timestamp::now();
    create_root(db, "root", &BookmarkRootGuid::Root.into(), 0, now)?;
    create_root(db, "menu", &BookmarkRootGuid::Menu.into(), 0, now)?;
    create_root(db, "toolbar", &BookmarkRootGuid::Toolbar.into(), 1, now)?;
    create_root(db, "unfiled", &BookmarkRootGuid::Unfiled.into(), 2, now)?;
    create_root(db, "mobile", &BookmarkRootGuid::Mobile.into(), 3, now)?;
    Ok(())
}

#[derive(Debug, Copy, Clone)]
pub enum BookmarkPosition {
    Specific(u32),
    Append,
}

/// Helpers to deal with managing the position correctly.

/// Updates the position of existing items so that the insertion of a child in
/// the position specified leaves all siblings with the correct position.
/// Returns the index the item should be inserted at.
fn resolve_pos_for_insert(
    db: &PlacesDb,
    pos: BookmarkPosition,
    parent: &RawBookmark,
) -> Result<u32> {
    Ok(match pos {
        BookmarkPosition::Specific(specified) => {
            let actual = min(specified, parent.child_count);
            // must reorder existing children.
            db.execute_named_cached(
                "UPDATE moz_bookmarks SET position = position + 1
                 WHERE parent = :parent_id
                 AND position >= :position",
                &[(":parent_id", &parent.row_id), (":position", &actual)],
            )?;
            actual
        }
        BookmarkPosition::Append => parent.child_count,
    })
}

/// Updates the position of existing items so that the deletion of a child
/// from the position specified leaves all siblings with the correct position.
fn update_pos_for_deletion(db: &PlacesDb, pos: u32, parent_id: RowId) -> Result<()> {
    db.execute_named_cached(
        "UPDATE moz_bookmarks SET position = position - 1
         WHERE parent = :parent
         AND position >= :position",
        &[(":parent", &parent_id), (":position", &pos)],
    )?;
    Ok(())
}

/// Updates the position of existing items when an item is being moved in the
/// same folder.
/// Returns what the position should be updated to.
fn update_pos_for_move(
    db: &PlacesDb,
    pos: BookmarkPosition,
    bm: &RawBookmark,
    parent: &RawBookmark,
) -> Result<u32> {
    assert_eq!(bm.parent_id.unwrap(), parent.row_id);
    // Note the additional -1's below are to account for the item already being
    // in the folder.
    let new_index = match pos {
        BookmarkPosition::Specific(specified) => min(specified, parent.child_count - 1),
        BookmarkPosition::Append => parent.child_count - 1,
    };
    db.execute_named_cached(
        "UPDATE moz_bookmarks
         SET position = CASE WHEN :new_index < :cur_index
            THEN position + 1
            ELSE position - 1
         END
         WHERE parent = :parent_id
         AND position BETWEEN :low_index AND :high_index",
        &[
            (":new_index", &new_index),
            (":cur_index", &bm.position),
            (":parent_id", &parent.row_id),
            (":low_index", &min(bm.position, new_index)),
            (":high_index", &max(bm.position, new_index)),
        ],
    )?;
    Ok(new_index)
}

/// Structures which can be used to insert a bookmark, folder or separator.
#[derive(Debug, Clone)]
pub struct InsertableBookmark {
    pub parent_guid: SyncGuid,
    pub position: BookmarkPosition,
    pub date_added: Option<Timestamp>,
    pub last_modified: Option<Timestamp>,
    pub guid: Option<SyncGuid>,
    pub url: Url,
    pub title: Option<String>,
}

impl From<InsertableBookmark> for InsertableItem {
    fn from(bmk: InsertableBookmark) -> Self {
        InsertableItem::Bookmark(bmk)
    }
}

#[derive(Debug, Clone)]
pub struct InsertableSeparator {
    pub parent_guid: SyncGuid,
    pub position: BookmarkPosition,
    pub date_added: Option<Timestamp>,
    pub last_modified: Option<Timestamp>,
    pub guid: Option<SyncGuid>,
}

impl From<InsertableSeparator> for InsertableItem {
    fn from(sep: InsertableSeparator) -> Self {
        InsertableItem::Separator(sep)
    }
}

#[derive(Debug, Clone)]
pub struct InsertableFolder {
    pub parent_guid: SyncGuid,
    pub position: BookmarkPosition,
    pub date_added: Option<Timestamp>,
    pub last_modified: Option<Timestamp>,
    pub guid: Option<SyncGuid>,
    pub title: Option<String>,
}

impl From<InsertableFolder> for InsertableItem {
    fn from(folder: InsertableFolder) -> Self {
        InsertableItem::Folder(folder)
    }
}

// The type used to insert the actual item.
#[derive(Debug, Clone)]
pub enum InsertableItem {
    Bookmark(InsertableBookmark),
    Separator(InsertableSeparator),
    Folder(InsertableFolder),
}

// We allow all "common" fields from the sub-types to be getters on the
// InsertableItem type.
macro_rules! impl_common_bookmark_getter {
    ($getter_name:ident, $T:ty) => {
        fn $getter_name(&self) -> &$T {
            match self {
                InsertableItem::Bookmark(b) => &b.$getter_name,
                InsertableItem::Separator(s) => &s.$getter_name,
                InsertableItem::Folder(f) => &f.$getter_name,
            }
        }
    };
}

impl InsertableItem {
    fn bookmark_type(&self) -> BookmarkType {
        match self {
            InsertableItem::Bookmark(_) => BookmarkType::Bookmark,
            InsertableItem::Separator(_) => BookmarkType::Separator,
            InsertableItem::Folder(_) => BookmarkType::Folder,
        }
    }
    impl_common_bookmark_getter!(parent_guid, SyncGuid);
    impl_common_bookmark_getter!(position, BookmarkPosition);
    impl_common_bookmark_getter!(date_added, Option<Timestamp>);
    impl_common_bookmark_getter!(last_modified, Option<Timestamp>);
    impl_common_bookmark_getter!(guid, Option<SyncGuid>);
}

pub fn insert_bookmark(db: &PlacesDb, bm: &InsertableItem) -> Result<SyncGuid> {
    let tx = db.unchecked_transaction()?;
    let result = insert_bookmark_in_tx(db, bm);
    super::delete_pending_temp_tables(db)?;
    match result {
        Ok(_) => tx.commit()?,
        Err(_) => tx.rollback()?,
    }
    result
}

fn maybe_truncate_title(t: &Option<String>) -> Option<&str> {
    use super::TITLE_LENGTH_MAX;
    use crate::util::slice_up_to;
    t.as_ref().map(|title| slice_up_to(title, TITLE_LENGTH_MAX))
}

fn insert_bookmark_in_tx(db: &PlacesDb, bm: &InsertableItem) -> Result<SyncGuid> {
    // find the row ID of the parent.
    if bm.parent_guid() == BookmarkRootGuid::Root {
        return Err(InvalidPlaceInfo::CannotUpdateRoot(BookmarkRootGuid::Root).into());
    }
    let parent_guid = bm.parent_guid();
    let parent = get_raw_bookmark(db, parent_guid)?
        .ok_or_else(|| InvalidPlaceInfo::NoSuchGuid(parent_guid.to_string()))?;
    if parent.bookmark_type != BookmarkType::Folder {
        return Err(InvalidPlaceInfo::InvalidParent(parent_guid.to_string()).into());
    }
    // Do the "position" dance.
    let position = resolve_pos_for_insert(db, *bm.position(), &parent)?;

    // Note that we could probably do this 'fk' work as a sub-query (although
    // markh isn't clear how we could perform the insert) - it probably doesn't
    // matter in practice though...
    let fk = match bm {
        InsertableItem::Bookmark(ref bm) => {
            let page_info = match fetch_page_info(db, &bm.url)? {
                Some(info) => info.page,
                None => new_page_info(db, &bm.url, None)?,
            };
            Some(page_info.row_id)
        }
        _ => None,
    };
    let sql = "INSERT INTO moz_bookmarks
              (fk, type, parent, position, title, dateAdded, lastModified,
               guid, syncStatus, syncChangeCounter) VALUES
              (:fk, :type, :parent, :position, :title, :dateAdded, :lastModified,
               :guid, :syncStatus, :syncChangeCounter)";

    let guid = bm.guid().clone().unwrap_or_else(SyncGuid::new);
    let date_added = bm.date_added().unwrap_or_else(Timestamp::now);
    // last_modified can't be before date_added
    let last_modified = max(
        bm.last_modified().unwrap_or_else(Timestamp::now),
        date_added,
    );

    let bookmark_type = bm.bookmark_type();
    match bm {
        InsertableItem::Bookmark(ref b) => {
            let title = maybe_truncate_title(&b.title);
            db.execute_named_cached(
                sql,
                &[
                    (":fk", &fk),
                    (":type", &bookmark_type),
                    (":parent", &parent.row_id),
                    (":position", &position),
                    (":title", &title),
                    (":dateAdded", &date_added),
                    (":lastModified", &last_modified),
                    (":guid", &guid),
                    (":syncStatus", &SyncStatus::New),
                    (":syncChangeCounter", &1),
                ],
            )?;
        }
        InsertableItem::Separator(ref _s) => {
            db.execute_named_cached(
                sql,
                &[
                    (":type", &bookmark_type),
                    (":parent", &parent.row_id),
                    (":position", &position),
                    (":dateAdded", &date_added),
                    (":lastModified", &last_modified),
                    (":guid", &guid),
                    (":syncStatus", &SyncStatus::New),
                    (":syncChangeCounter", &1),
                ],
            )?;
        }
        InsertableItem::Folder(ref f) => {
            let title = maybe_truncate_title(&f.title);
            db.execute_named_cached(
                sql,
                &[
                    (":type", &bookmark_type),
                    (":parent", &parent.row_id),
                    (":title", &title),
                    (":position", &position),
                    (":dateAdded", &date_added),
                    (":lastModified", &last_modified),
                    (":guid", &guid),
                    (":syncStatus", &SyncStatus::New),
                    (":syncChangeCounter", &1),
                ],
            )?;
        }
    };

    // Bump the parent's change counter.
    let sql_counter = "
        UPDATE moz_bookmarks SET syncChangeCounter = syncChangeCounter + 1
        WHERE id = :parent_id";
    db.execute_named_cached(sql_counter, &[(":parent_id", &parent.row_id)])?;

    Ok(guid)
}

/// Delete the specified bookmark. Returns true if a bookmark with the guid
/// existed and was deleted, false otherwise.
pub fn delete_bookmark(db: &PlacesDb, guid: &SyncGuid) -> Result<bool> {
    let tx = db.unchecked_transaction()?;
    let result = delete_bookmark_in_tx(db, guid);
    match result {
        Ok(_) => tx.commit()?,
        Err(_) => tx.rollback()?,
    }
    result
}

fn delete_bookmark_in_tx(db: &PlacesDb, guid: &SyncGuid) -> Result<bool> {
    // Can't delete a root.
    if let Some(root) = guid.as_root() {
        return Err(InvalidPlaceInfo::CannotUpdateRoot(root).into());
    }
    let record = match get_raw_bookmark(db, guid)? {
        Some(r) => r,
        None => {
            log::debug!("Can't delete bookmark '{:?}' as it doesn't exist", guid);
            return Ok(false);
        }
    };
    // There's an argument to be made here that we should still honor the
    // deletion in the case of this corruption, since it would be fixed by
    // performing the deletion, and the user wants it gone...
    let record_parent_id = record
        .parent_id
        .ok_or_else(|| Corruption::NonRootWithoutParent(guid.to_string()))?;
    // must reorder existing children.
    update_pos_for_deletion(db, record.position, record_parent_id)?;
    // and delete - children are recursively deleted.
    db.execute_named_cached(
        "DELETE from moz_bookmarks WHERE id = :id",
        &[(":id", &record.row_id)],
    )?;
    super::delete_pending_temp_tables(db)?;
    Ok(true)
}

/// Support for modifying bookmarks, including changing the location in
/// the tree.

// Used to specify how the location of the item in the tree should be updated.
#[derive(Debug, Clone)]
pub enum UpdateTreeLocation {
    None,                               // no change
    Position(BookmarkPosition),         // new position in the same folder.
    Parent(SyncGuid, BookmarkPosition), // new parent
}

impl Default for UpdateTreeLocation {
    fn default() -> Self {
        UpdateTreeLocation::None
    }
}

/// Structures which can be used to update a bookmark, folder or separator.
/// Almost all fields are Option<>-like, with None meaning "do not change".
/// Many fields which can't be changed by our public API are omitted (eg,
/// guid, date_added, last_modified, etc)
#[derive(Debug, Clone, Default)]
pub struct UpdatableBookmark {
    pub location: UpdateTreeLocation,
    pub url: Option<Url>,
    pub title: Option<String>,
}

impl From<UpdatableBookmark> for UpdatableItem {
    fn from(bmk: UpdatableBookmark) -> Self {
        UpdatableItem::Bookmark(bmk)
    }
}

#[derive(Debug, Clone)]
pub struct UpdatableSeparator {
    pub location: UpdateTreeLocation,
}

impl From<UpdatableSeparator> for UpdatableItem {
    fn from(sep: UpdatableSeparator) -> Self {
        UpdatableItem::Separator(sep)
    }
}

#[derive(Debug, Clone, Default)]
pub struct UpdatableFolder {
    pub location: UpdateTreeLocation,
    pub title: Option<String>,
}

impl From<UpdatableFolder> for UpdatableItem {
    fn from(folder: UpdatableFolder) -> Self {
        UpdatableItem::Folder(folder)
    }
}

// The type used to update the actual item.
#[derive(Debug, Clone)]
pub enum UpdatableItem {
    Bookmark(UpdatableBookmark),
    Separator(UpdatableSeparator),
    Folder(UpdatableFolder),
}

impl UpdatableItem {
    fn bookmark_type(&self) -> BookmarkType {
        match self {
            UpdatableItem::Bookmark(_) => BookmarkType::Bookmark,
            UpdatableItem::Separator(_) => BookmarkType::Separator,
            UpdatableItem::Folder(_) => BookmarkType::Folder,
        }
    }

    pub fn location(&self) -> &UpdateTreeLocation {
        match self {
            UpdatableItem::Bookmark(b) => &b.location,
            UpdatableItem::Separator(s) => &s.location,
            UpdatableItem::Folder(f) => &f.location,
        }
    }
}
pub fn update_bookmark(db: &PlacesDb, guid: &SyncGuid, item: &UpdatableItem) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    let result = update_bookmark_in_tx(db, guid, item);
    // Note: `tx` automatically rolls back on drop if we don't commit
    tx.commit()?;
    result
}

fn update_bookmark_in_tx(db: &PlacesDb, guid: &SyncGuid, item: &UpdatableItem) -> Result<()> {
    if guid.is_root() {
        return Err(InvalidPlaceInfo::CannotUpdateRoot(BookmarkRootGuid::Root).into());
    }
    let existing = get_raw_bookmark(db, guid)?
        .ok_or_else(|| InvalidPlaceInfo::NoSuchGuid(guid.to_string()))?;
    let existing_parent_guid = existing
        .parent_guid
        .as_ref()
        .ok_or_else(|| Corruption::NonRootWithoutParent(guid.to_string()))?;

    let existing_parent_id = existing
        .parent_id
        .ok_or_else(|| Corruption::NoParent(guid.to_string(), existing_parent_guid.to_string()))?;

    if existing.bookmark_type != item.bookmark_type() {
        return Err(InvalidPlaceInfo::MismatchedBookmarkType(
            existing.bookmark_type as u8,
            item.bookmark_type() as u8,
        )
        .into());
    }

    let update_old_parent_status;
    let update_new_parent_status;
    // to make our life easier we update every field, using existing when
    // no value is specified.
    let parent_id;
    let position;
    match item.location() {
        UpdateTreeLocation::None => {
            parent_id = existing_parent_id;
            position = existing.position;
            update_old_parent_status = false;
            update_new_parent_status = false;
        }
        UpdateTreeLocation::Position(pos) => {
            parent_id = existing_parent_id;
            update_old_parent_status = true;
            update_new_parent_status = false;
            let parent = get_raw_bookmark(db, existing_parent_guid)?.ok_or_else(|| {
                Corruption::NoParent(guid.to_string(), existing_parent_guid.to_string())
            })?;
            position = update_pos_for_move(db, *pos, &existing, &parent)?;
        }
        UpdateTreeLocation::Parent(new_parent_guid, pos) => {
            if new_parent_guid == BookmarkRootGuid::Root {
                return Err(InvalidPlaceInfo::CannotUpdateRoot(BookmarkRootGuid::Root).into());
            }
            let new_parent = get_raw_bookmark(db, &new_parent_guid)?
                .ok_or_else(|| InvalidPlaceInfo::NoSuchGuid(new_parent_guid.to_string()))?;
            if new_parent.bookmark_type != BookmarkType::Folder {
                return Err(InvalidPlaceInfo::InvalidParent(new_parent_guid.to_string()).into());
            }
            parent_id = new_parent.row_id;
            update_old_parent_status = true;
            update_new_parent_status = true;
            let existing_parent = get_raw_bookmark(db, existing_parent_guid)?.ok_or_else(|| {
                Corruption::NoParent(guid.to_string(), existing_parent_guid.to_string())
            })?;
            update_pos_for_deletion(db, existing.position, existing_parent.row_id)?;
            position = resolve_pos_for_insert(db, *pos, &new_parent)?;
        }
    };
    let place_id = match item {
        UpdatableItem::Bookmark(b) => match &b.url {
            None => existing.place_id,
            Some(url) => {
                let page_info = match fetch_page_info(db, &url)? {
                    Some(info) => info.page,
                    None => new_page_info(db, &url, None)?,
                };
                Some(page_info.row_id)
            }
        },
        _ => {
            // Updating a non-bookmark item, so the existing item must not
            // have a place_id
            assert_eq!(existing.place_id, None);
            None
        }
    };
    // While we could let the SQL take care of being clever about the update
    // via, say `title = NULLIF(IFNULL(:title, title), '')`, this code needs
    // to know if it changed so the sync counter can be managed correctly.
    let update_title = match item {
        UpdatableItem::Bookmark(b) => &b.title,
        UpdatableItem::Folder(f) => &f.title,
        UpdatableItem::Separator(_) => &None,
    };

    let title: Option<String> = match update_title {
        None => existing.title.clone(),
        // We don't differentiate between null and the empty string for title,
        // just like desktop doesn't post bug 1360872, hence an empty string
        // means "set to null".
        Some(val) => {
            if val.is_empty() {
                None
            } else {
                Some(val.clone())
            }
        }
    };

    let change_incr = title != existing.title || place_id != existing.place_id;

    let now = Timestamp::now();

    let sql = "
        UPDATE moz_bookmarks SET
            fk = :fk,
            parent = :parent,
            position = :position,
            title = :title,
            lastModified = :now,
            syncChangeCounter = syncChangeCounter + :change_incr
        WHERE id = :id";

    db.execute_named_cached(
        sql,
        &[
            (":fk", &place_id),
            (":parent", &parent_id),
            (":position", &position),
            (":title", &maybe_truncate_title(&title)),
            (":now", &now),
            (":change_incr", &(change_incr as u32)),
            (":id", &existing.row_id),
        ],
    )?;

    let sql_counter = "
        UPDATE moz_bookmarks SET syncChangeCounter = syncChangeCounter + 1
        WHERE id = :parent_id";

    // The lastModified of the existing parent ancestors (which may still be
    // the current parent) is always updated, even if the change counter for it
    // isn't.
    set_ancestors_last_modified(db, existing_parent_id, now)?;
    if update_old_parent_status {
        db.execute_named_cached(sql_counter, &[(":parent_id", &existing_parent_id)])?;
    }
    if update_new_parent_status {
        set_ancestors_last_modified(db, parent_id, now)?;
        db.execute_named_cached(sql_counter, &[(":parent_id", &parent_id)])?;
    }
    Ok(())
}

fn set_ancestors_last_modified(db: &PlacesDb, parent_id: RowId, time: Timestamp) -> Result<()> {
    let sql = "
        WITH RECURSIVE
        ancestors(aid) AS (
            SELECT :parent_id
            UNION ALL
            SELECT parent FROM moz_bookmarks
            JOIN ancestors ON id = aid
            WHERE type = :type
        )
        UPDATE moz_bookmarks SET lastModified = :time
        WHERE id IN ancestors
    ";
    db.execute_named_cached(
        sql,
        &[
            (":parent_id", &parent_id),
            (":type", &(BookmarkType::Folder as u8)),
            (":time", &time),
        ],
    )?;
    Ok(())
}

/// Support for inserting and fetching a tree. Same limitations as desktop.
/// Note that the guids are optional when inserting a tree. They will always
/// have values when fetching it.

// For testing purposes we implement PartialEq, such that optional fields are
// ignored in the comparison. This allows tests to construct a tree with
// missing fields and be able to compare against a tree with all fields (such
// as one exported from the DB)
#[cfg(test)]
fn cmp_options<T: PartialEq>(s: &Option<T>, o: &Option<T>) -> bool {
    match (s, o) {
        (None, None) => true,
        (None, Some(_)) => true,
        (Some(_), None) => true,
        (s, o) => s == o,
    }
}

#[derive(Debug)]
pub struct BookmarkNode {
    pub guid: Option<SyncGuid>,
    pub date_added: Option<Timestamp>,
    pub last_modified: Option<Timestamp>,
    pub title: Option<String>,
    pub url: Url,
}

impl From<BookmarkNode> for BookmarkTreeNode {
    fn from(node: BookmarkNode) -> Self {
        BookmarkTreeNode::Bookmark(node)
    }
}

#[cfg(test)]
impl PartialEq for BookmarkNode {
    fn eq(&self, other: &BookmarkNode) -> bool {
        cmp_options(&self.guid, &other.guid)
            && cmp_options(&self.date_added, &other.date_added)
            && cmp_options(&self.last_modified, &other.last_modified)
            && cmp_options(&self.title, &other.title)
            && self.url == other.url
    }
}

#[derive(Debug, Default)]
pub struct SeparatorNode {
    pub guid: Option<SyncGuid>,
    pub date_added: Option<Timestamp>,
    pub last_modified: Option<Timestamp>,
}

impl From<SeparatorNode> for BookmarkTreeNode {
    fn from(node: SeparatorNode) -> Self {
        BookmarkTreeNode::Separator(node)
    }
}

#[cfg(test)]
impl PartialEq for SeparatorNode {
    fn eq(&self, other: &SeparatorNode) -> bool {
        cmp_options(&self.guid, &other.guid)
            && cmp_options(&self.date_added, &other.date_added)
            && cmp_options(&self.last_modified, &other.last_modified)
    }
}

#[derive(Debug, Default)]
pub struct FolderNode {
    pub guid: Option<SyncGuid>,
    pub date_added: Option<Timestamp>,
    pub last_modified: Option<Timestamp>,
    pub title: Option<String>,
    pub children: Vec<BookmarkTreeNode>,
}

impl From<FolderNode> for BookmarkTreeNode {
    fn from(node: FolderNode) -> Self {
        BookmarkTreeNode::Folder(node)
    }
}

#[cfg(test)]
impl PartialEq for FolderNode {
    fn eq(&self, other: &FolderNode) -> bool {
        cmp_options(&self.guid, &other.guid)
            && cmp_options(&self.date_added, &other.date_added)
            && cmp_options(&self.last_modified, &other.last_modified)
            && cmp_options(&self.title, &other.title)
            && self.children == other.children
    }
}

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub enum BookmarkTreeNode {
    Bookmark(BookmarkNode),
    Separator(SeparatorNode),
    Folder(FolderNode),
}

impl BookmarkTreeNode {
    pub fn node_type(&self) -> BookmarkType {
        match self {
            BookmarkTreeNode::Bookmark(_) => BookmarkType::Bookmark,
            BookmarkTreeNode::Folder(_) => BookmarkType::Folder,
            BookmarkTreeNode::Separator(_) => BookmarkType::Separator,
        }
    }

    pub fn guid(&self) -> &SyncGuid {
        let guid = match self {
            BookmarkTreeNode::Bookmark(b) => b.guid.as_ref(),
            BookmarkTreeNode::Folder(f) => f.guid.as_ref(),
            BookmarkTreeNode::Separator(s) => s.guid.as_ref(),
        };
        // Can this happen? Why is this an Option?
        guid.expect("Missing guid?")
    }

    pub fn created_modified(&self) -> (Timestamp, Timestamp) {
        let (created, modified) = match self {
            BookmarkTreeNode::Bookmark(b) => (b.date_added, b.last_modified),
            BookmarkTreeNode::Folder(f) => (f.date_added, f.last_modified),
            BookmarkTreeNode::Separator(s) => (s.date_added, s.last_modified),
        };
        (
            created.unwrap_or_else(Timestamp::now),
            modified.unwrap_or_else(Timestamp::now),
        )
    }
}

// Serde makes it tricky to serialize what we need here - a 'type' from the
// enum and then a flattened variant struct. So we gotta do it manually.
impl Serialize for BookmarkTreeNode {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("BookmarkTreeNode", 2)?;
        match self {
            BookmarkTreeNode::Bookmark(b) => {
                state.serialize_field("type", &BookmarkType::Bookmark)?;
                state.serialize_field("guid", &b.guid)?;
                state.serialize_field("date_added", &b.date_added)?;
                state.serialize_field("last_modified", &b.last_modified)?;
                state.serialize_field("title", &b.title)?;
                state.serialize_field("url", &b.url.to_string())?;
            }
            BookmarkTreeNode::Separator(s) => {
                state.serialize_field("type", &BookmarkType::Separator)?;
                state.serialize_field("guid", &s.guid)?;
                state.serialize_field("date_added", &s.date_added)?;
                state.serialize_field("last_modified", &s.last_modified)?;
            }
            BookmarkTreeNode::Folder(f) => {
                state.serialize_field("type", &BookmarkType::Folder)?;
                state.serialize_field("guid", &f.guid)?;
                state.serialize_field("date_added", &f.date_added)?;
                state.serialize_field("last_modified", &f.last_modified)?;
                state.serialize_field("title", &f.title)?;
                state.serialize_field("children", &f.children)?;
            }
        };
        state.end()
    }
}

impl<'de> Deserialize<'de> for BookmarkTreeNode {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // *sob* - a union of fields we post-process.
        #[derive(Debug, Default, Deserialize)]
        #[serde(default)]
        struct Mapping {
            #[serde(rename = "type")]
            bookmark_type: u8,
            guid: Option<SyncGuid>,
            date_added: Option<Timestamp>,
            last_modified: Option<Timestamp>,
            title: Option<String>,
            url: Option<String>,
            children: Vec<BookmarkTreeNode>,
        }
        let m = Mapping::deserialize(deserializer)?;

        let url = m.url.as_ref().and_then(|u| match Url::parse(u) {
            Err(e) => {
                log::warn!(
                    "ignoring invalid url for {}: {:?}",
                    m.guid
                        .as_ref()
                        .map(|guid| guid.as_ref())
                        .unwrap_or("<no guid>"),
                    e
                );
                None
            }
            Ok(parsed) => Some(parsed),
        });

        let bookmark_type = BookmarkType::from_u8_with_valid_url(m.bookmark_type, || url.is_some());
        Ok(match bookmark_type {
            BookmarkType::Bookmark => BookmarkNode {
                guid: m.guid,
                date_added: m.date_added,
                last_modified: m.last_modified,
                title: m.title,
                url: url.unwrap(),
            }
            .into(),
            BookmarkType::Separator => SeparatorNode {
                guid: m.guid,
                date_added: m.date_added,
                last_modified: m.last_modified,
            }
            .into(),
            BookmarkType::Folder => FolderNode {
                guid: m.guid,
                date_added: m.date_added,
                last_modified: m.last_modified,
                title: m.title,
                children: m.children,
            }
            .into(),
        })
    }
}

#[cfg(test)]
mod test_serialize {
    use super::*;

    #[test]
    fn test_tree_serialize() -> Result<()> {
        let guid = SyncGuid::new();
        let tree = BookmarkTreeNode::Folder(FolderNode {
            guid: Some(guid.clone()),
            date_added: None,
            last_modified: None,
            title: None,
            children: vec![BookmarkTreeNode::Bookmark(BookmarkNode {
                guid: None,
                date_added: None,
                last_modified: None,
                title: Some("the bookmark".into()),
                url: Url::parse("https://www.example.com")?,
            })],
        });
        // round-trip the tree via serde.
        let json = serde_json::to_string_pretty(&tree)?;
        let deser: BookmarkTreeNode = serde_json::from_str(&json)?;
        assert_eq!(tree, deser);
        // and check against the simplest json repr of the tree, which checks
        // our PartialEq implementation.
        let jtree = json!({
            "type": 2,
            "guid": &guid,
            "children" : [
                {
                    "type": 1,
                    "title": "the bookmark",
                    "url": "https://www.example.com/"
                }
            ]
        });
        let deser_tree: BookmarkTreeNode = serde_json::from_value(jtree).expect("should deser");
        assert_eq!(tree, deser_tree);
        Ok(())
    }

    #[test]
    fn test_tree_invalid() -> Result<()> {
        let jtree = json!({
            "type": 2,
            "children" : [
                {
                    "type": 1,
                    "title": "bookmark with invalid URL",
                    "url": "invalid_url"
                },
                {
                    "type": 1,
                    "title": "bookmark with missing URL",
                },
                {
                    "title": "bookmark with missing type, no URL",
                },
                {
                    "title": "bookmark with missing type, valid URL",
                    "url": "http://example.com"
                },

            ]
        });
        let deser_tree: BookmarkTreeNode = serde_json::from_value(jtree).expect("should deser");
        let folder = match deser_tree {
            BookmarkTreeNode::Folder(f) => f,
            _ => panic!("must be a folder"),
        };

        let children = folder.children;
        assert_eq!(children.len(), 4);

        assert!(match &children[0] {
            BookmarkTreeNode::Folder(f) => f.title == Some("bookmark with invalid URL".to_string()),
            _ => false,
        });
        assert!(match &children[1] {
            BookmarkTreeNode::Folder(f) => f.title == Some("bookmark with missing URL".to_string()),
            _ => false,
        });
        assert!(match &children[2] {
            BookmarkTreeNode::Folder(f) => {
                f.title == Some("bookmark with missing type, no URL".to_string())
            }
            _ => false,
        });
        assert!(match &children[3] {
            BookmarkTreeNode::Bookmark(b) => {
                b.title == Some("bookmark with missing type, valid URL".to_string())
            }
            _ => false,
        });

        Ok(())
    }

}

fn add_subtree_infos(parent: &SyncGuid, tree: &FolderNode, insert_infos: &mut Vec<InsertableItem>) {
    // TODO: track last modified? Like desktop, we should probably have
    // the default values passed in so the entire tree has consistent
    // timestamps.
    let default_when = Some(Timestamp::now());
    insert_infos.reserve(tree.children.len());
    for child in &tree.children {
        match child {
            BookmarkTreeNode::Bookmark(b) => insert_infos.push(
                InsertableBookmark {
                    parent_guid: parent.clone(),
                    position: BookmarkPosition::Append,
                    date_added: b.date_added.or(default_when),
                    last_modified: b.last_modified.or(default_when),
                    guid: b.guid.clone(),
                    url: b.url.clone(),
                    title: b.title.clone(),
                }
                .into(),
            ),
            BookmarkTreeNode::Separator(s) => insert_infos.push(
                InsertableSeparator {
                    parent_guid: parent.clone(),
                    position: BookmarkPosition::Append,
                    date_added: s.date_added.or(default_when),
                    last_modified: s.last_modified.or(default_when),
                    guid: s.guid.clone(),
                }
                .into(),
            ),
            BookmarkTreeNode::Folder(f) => {
                let my_guid = f.guid.clone().unwrap_or_else(SyncGuid::new);
                // must add the folder before we recurse into children.
                insert_infos.push(
                    InsertableFolder {
                        parent_guid: parent.clone(),
                        position: BookmarkPosition::Append,
                        date_added: f.date_added.or(default_when),
                        last_modified: f.last_modified.or(default_when),
                        guid: Some(my_guid.clone()),
                        title: f.title.clone(),
                    }
                    .into(),
                );
                add_subtree_infos(&my_guid, &f, insert_infos);
            }
        };
    }
}

pub fn insert_tree(db: &PlacesDb, tree: &FolderNode) -> Result<()> {
    let parent_guid = match &tree.guid {
        Some(guid) => guid,
        None => return Err(InvalidPlaceInfo::InvalidParent("<no guid>".into()).into()),
    };

    let mut insert_infos: Vec<InsertableItem> = Vec::new();
    add_subtree_infos(&parent_guid, tree, &mut insert_infos);
    log::info!("insert_tree inserting {} records", insert_infos.len());
    let tx = db.unchecked_transaction()?;

    for insertable in insert_infos {
        insert_bookmark_in_tx(db, &insertable)?;
    }
    super::delete_pending_temp_tables(db)?;
    tx.commit()?;
    Ok(())
}

#[derive(Debug)]
struct FetchedTreeRow {
    level: u32,
    id: RowId,
    guid: SyncGuid,
    // parent and parent_guid are Option<> only to handle the root - we would
    // assert but they aren't currently used.
    parent: Option<RowId>,
    parent_guid: Option<SyncGuid>,
    node_type: BookmarkType,
    position: u32,
    title: Option<String>,
    date_added: Timestamp,
    last_modified: Timestamp,
    url: Option<String>,
}

impl FetchedTreeRow {
    pub fn from_row(row: &Row) -> Result<Self> {
        let url = row.get_checked::<_, Option<String>>("url")?;
        Ok(Self {
            level: row.get_checked("level")?,
            id: row.get_checked::<_, RowId>("id")?,
            guid: SyncGuid(row.get_checked::<_, String>("guid")?),
            parent: row.get_checked::<_, Option<RowId>>("parent")?,
            parent_guid: row
                .get_checked::<_, Option<String>>("parentGuid")?
                .map(SyncGuid),
            node_type: BookmarkType::from_u8_with_valid_url(
                row.get_checked::<_, u8>("type")?,
                || url.is_some(),
            ),
            position: row.get_checked("position")?,
            title: row.get_checked::<_, Option<String>>("title")?,
            date_added: row.get_checked("dateAdded")?,
            last_modified: row.get_checked("lastModified")?,
            url,
        })
    }
}

fn inflate(
    parent: &mut BookmarkTreeNode,
    pseudo_tree: &mut HashMap<SyncGuid, Vec<BookmarkTreeNode>>,
) {
    if let BookmarkTreeNode::Folder(parent) = parent {
        if let Some(children) = parent
            .guid
            .as_ref()
            .and_then(|guid| pseudo_tree.remove(guid))
        {
            parent.children = children;
            for mut child in &mut parent.children {
                inflate(&mut child, pseudo_tree);
            }
        }
    }
}

/// Fetch the tree starting at the specified folder guid.
/// Returns a BookmarkTreeNode::Folder(_)
pub fn fetch_tree(db: &PlacesDb, item_guid: &SyncGuid) -> Result<Option<BookmarkTreeNode>> {
    // XXX - this needs additional work for tags - unlike desktop, there's no
    // "tags" folder, but instead a couple of tables to join on.
    let sql = r#"
        WITH RECURSIVE
        descendants(fk, level, type, id, guid, parent, parentGuid, position,
                    title, dateAdded, lastModified) AS (
          SELECT b1.fk, 0, b1.type, b1.id, b1.guid, b1.parent,
                 (SELECT guid FROM moz_bookmarks WHERE id = b1.parent),
                 b1.position, b1.title, b1.dateAdded, b1.lastModified
          FROM moz_bookmarks b1 WHERE b1.guid=:item_guid
          UNION ALL
          SELECT b2.fk, level + 1, b2.type, b2.id, b2.guid, b2.parent,
                 descendants.guid, b2.position, b2.title, b2.dateAdded,
                 b2.lastModified
          FROM moz_bookmarks b2
          JOIN descendants ON b2.parent = descendants.id) -- AND b2.id <> :tags_folder)
        SELECT d.level, d.id, d.guid, d.parent, d.parentGuid, d.type,
               d.position, NULLIF(d.title, '') AS title, d.dateAdded,
               d.lastModified, h.url
--               (SELECT icon_url FROM moz_icons i
--                      JOIN moz_icons_to_pages ON icon_id = i.id
--                      JOIN moz_pages_w_icons pi ON page_id = pi.id
--                      WHERE pi.page_url_hash = hash(h.url) AND pi.page_url = h.url
--                      ORDER BY width DESC LIMIT 1) AS iconuri,
--               (SELECT GROUP_CONCAT(t.title, ',')
--                FROM moz_bookmarks b2
--                JOIN moz_bookmarks t ON t.id = +b2.parent AND t.parent = :tags_folder
--                WHERE b2.fk = h.id
--               ) AS tags,
--               EXISTS (SELECT 1 FROM moz_items_annos
--                       WHERE item_id = d.id LIMIT 1) AS has_annos,
--               (SELECT a.content FROM moz_annos a
--                JOIN moz_anno_attributes n ON a.anno_attribute_id = n.id
--                WHERE place_id = h.id AND n.name = :charset_anno
--               ) AS charset
        FROM descendants d
        LEFT JOIN moz_bookmarks b3 ON b3.id = d.parent
        LEFT JOIN moz_places h ON h.id = d.fk
        ORDER BY d.level, d.parent, d.position"#;

    let scope = db.begin_interrupt_scope();

    let mut stmt = db.conn().prepare(sql)?;

    let mut results =
        stmt.query_and_then_named(&[(":item_guid", item_guid)], FetchedTreeRow::from_row)?;

    // The first row in the result set is always the root of our tree.
    let mut root = match results.next() {
        Some(result) => {
            let row = result?;
            FolderNode {
                guid: Some(row.guid.clone()),
                date_added: Some(row.date_added),
                last_modified: Some(row.last_modified),
                title: row.title.clone(),
                children: Vec::new(),
            }
            .into()
        }
        None => return Ok(None),
    };

    scope.err_if_interrupted()?;
    // For all remaining rows, build a pseudo-tree that maps parent GUIDs to
    // ordered children. We need this intermediate step because SQLite returns
    // results in level order, so we'll see a node's siblings and cousins (same
    // level, but different parents) before any of their descendants.
    let mut pseudo_tree: HashMap<SyncGuid, Vec<BookmarkTreeNode>> = HashMap::new();
    for result in results {
        let row = result?;
        scope.err_if_interrupted()?;
        let node = match row.node_type {
            BookmarkType::Bookmark => match &row.url {
                Some(url_str) => match Url::parse(&url_str) {
                    Ok(url) => BookmarkNode {
                        guid: Some(row.guid.clone()),
                        date_added: Some(row.date_added),
                        last_modified: Some(row.last_modified),
                        title: row.title.clone(),
                        url,
                    }
                    .into(),
                    Err(e) => {
                        log::warn!(
                            "ignoring malformed bookmark {} - invalid URL: {:?}",
                            row.guid,
                            e
                        );
                        continue;
                    }
                },
                None => {
                    log::warn!("ignoring malformed bookmark {} - no URL", row.guid);
                    continue;
                }
            },
            BookmarkType::Separator => SeparatorNode {
                guid: Some(row.guid.clone()),
                date_added: Some(row.date_added),
                last_modified: Some(row.last_modified),
            }
            .into(),
            BookmarkType::Folder => FolderNode {
                guid: Some(row.guid.clone()),
                date_added: Some(row.date_added),
                last_modified: Some(row.last_modified),
                title: row.title.clone(),
                children: Vec::new(),
            }
            .into(),
        };
        if let Some(parent_guid) = row.parent_guid.as_ref().cloned() {
            let children = pseudo_tree.entry(parent_guid).or_default();
            children.push(node);
        }
    }

    // Finally, inflate our tree.
    inflate(&mut root, &mut pseudo_tree);
    Ok(Some(root))
}

/// A "raw" bookmark - a representation of the row and some summary fields.
#[derive(Debug)]
struct RawBookmark {
    place_id: Option<RowId>,
    row_id: RowId,
    bookmark_type: BookmarkType,
    parent_id: Option<RowId>,
    parent_guid: Option<SyncGuid>,
    position: u32,
    title: Option<String>,
    url: Option<Url>,
    date_added: Timestamp,
    date_modified: Timestamp,
    guid: SyncGuid,
    sync_status: SyncStatus,
    sync_change_counter: u32,
    child_count: u32,
    grandparent_id: Option<RowId>,
}

impl RawBookmark {
    pub fn from_row(row: &Row) -> Result<Self> {
        let place_id = row.get_checked::<_, Option<RowId>>("fk")?;
        Ok(Self {
            row_id: row.get_checked("_id")?,
            place_id,
            bookmark_type: BookmarkType::from_u8_with_valid_url(
                row.get_checked::<_, u8>("type")?,
                || place_id.is_some(),
            ),
            parent_id: row.get_checked("_parentId")?,
            parent_guid: row.get_checked("parentGuid")?,
            position: row.get_checked("position")?,
            title: row.get_checked::<_, Option<String>>("title")?,
            url: match row.get_checked::<_, Option<String>>("url")? {
                Some(s) => Some(Url::parse(&s)?),
                None => None,
            },
            date_added: row.get_checked("dateAdded")?,
            date_modified: row.get_checked("lastModified")?,
            guid: SyncGuid(row.get_checked::<_, String>("guid")?),
            sync_status: SyncStatus::from_u8(row.get_checked::<_, u8>("_syncStatus")?),
            sync_change_counter: row
                .get_checked::<_, Option<u32>>("syncChangeCounter")?
                .unwrap_or_default(),
            child_count: row.get_checked("_childCount")?,
            grandparent_id: row.get_checked("_grandparentId")?,
        })
    }
}

/// sql is based on fetchBookmark() in Desktop's Bookmarks.jsm, with 'fk' added
/// and title's NULLIF handling.
const RAW_BOOKMARK_SQL: &str = "
    SELECT
        b.guid,
        p.guid AS parentGuid,
        b.position,
        b.dateAdded,
        b.lastModified,
        b.type,
        -- Note we return null for titles with an empty string.
        NULLIF(b.title, '') AS title,
        h.url AS url,
        b.id AS _id,
        b.parent AS _parentId,
        (SELECT count(*) FROM moz_bookmarks WHERE parent = b.id) AS _childCount,
        p.parent AS _grandParentId,
        b.syncStatus AS _syncStatus,
        -- the columns below don't appear in the desktop query
        b.fk,
        b.syncChangeCounter
    FROM moz_bookmarks b
    LEFT JOIN moz_bookmarks p ON p.id = b.parent
    LEFT JOIN moz_places h ON h.id = b.fk
";

fn get_raw_bookmark(db: &PlacesDb, guid: &SyncGuid) -> Result<Option<RawBookmark>> {
    // sql is based on fetchBookmark() in Desktop's Bookmarks.jsm, with 'fk' added
    // and title's NULLIF handling.
    Ok(db.try_query_row(
        &format!("{} WHERE b.guid = :guid", RAW_BOOKMARK_SQL),
        &[(":guid", guid)],
        RawBookmark::from_row,
        true,
    )?)
}

fn get_raw_bookmarks_for_url(db: &PlacesDb, url: &Url) -> Result<Vec<RawBookmark>> {
    Ok(db.query_rows_into_cached(
        &format!(
            "{} WHERE h.url_hash = hash(:url) AND h.url = :url",
            RAW_BOOKMARK_SQL
        ),
        &[(":url", &url.as_str())],
        RawBookmark::from_row,
    )?)
}
fn get_raw_bookmarks_with_parent(db: &impl ConnExt, parent: RowId) -> Result<Vec<RawBookmark>> {
    Ok(db.query_rows_into_cached(
        &format!(
            "{} WHERE b.parent = :parent ORDER BY b.position ASC",
            RAW_BOOKMARK_SQL
        ),
        &[(":parent", &parent)],
        RawBookmark::from_row,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::test::new_mem_connection;
    use crate::db::PlacesDb;
    use pretty_assertions::assert_eq;
    use rusqlite::NO_PARAMS;
    use serde_json::Value;
    use std::collections::HashSet;

    pub(super) fn insert_json_tree(conn: &PlacesDb, jtree: Value) {
        let tree: BookmarkTreeNode = serde_json::from_value(jtree).expect("should be valid");
        let folder_node = match tree {
            BookmarkTreeNode::Folder(folder_node) => folder_node,
            _ => panic!("must be a folder"),
        };
        insert_tree(conn, &folder_node).expect("should insert");
    }

    fn assert_json_tree(conn: &PlacesDb, folder: &SyncGuid, expected: Value) {
        let fetched = fetch_tree(conn, folder).expect("should work").unwrap();
        let deser_tree: BookmarkTreeNode = serde_json::from_value(expected).unwrap();
        assert_eq!(fetched, deser_tree);
        // and while checking the tree, check positions are correct.
        check_positions(&conn);
    }

    fn get_pos(conn: &PlacesDb, guid: &SyncGuid) -> u32 {
        get_raw_bookmark(conn, guid)
            .expect("should work")
            .unwrap()
            .position
    }

    // check the positions for children in a folder are "correct" in that
    // the first child has a value of zero, etc - ie, this will fail if there
    // are holes or duplicates in the position values.
    // Clever implementation stolen from desktop.
    fn check_positions(conn: &PlacesDb) {
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

    #[test]
    fn test_insert() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = new_mem_connection();
        let url = Url::parse("https://www.example.com")?;

        conn.execute("UPDATE moz_bookmarks SET syncChangeCounter = 0", NO_PARAMS)
            .expect("should work");

        let bm = InsertableItem::Bookmark(InsertableBookmark {
            parent_guid: BookmarkRootGuid::Unfiled.into(),
            position: BookmarkPosition::Append,
            date_added: None,
            last_modified: None,
            guid: None,
            url: url.clone(),
            title: Some("the title".into()),
        });
        let guid = insert_bookmark(&conn, &bm)?;

        // re-fetch it.
        let rb = get_raw_bookmark(&conn, &guid)?.expect("should get the bookmark");

        assert!(rb.place_id.is_some());
        assert_eq!(rb.bookmark_type, BookmarkType::Bookmark);
        assert_eq!(rb.parent_guid.unwrap(), BookmarkRootGuid::Unfiled);
        assert_eq!(rb.position, 0);
        assert_eq!(rb.title, Some("the title".into()));
        assert_eq!(rb.url, Some(url));
        assert_eq!(rb.sync_status, SyncStatus::New);
        assert_eq!(rb.sync_change_counter, 1);
        assert_eq!(rb.child_count, 0);

        let unfiled = get_raw_bookmark(&conn, &BookmarkRootGuid::Unfiled.as_guid())?
            .expect("should get unfiled");
        assert_eq!(unfiled.sync_change_counter, 1);

        Ok(())
    }

    #[test]
    fn test_insert_titles() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = new_mem_connection();
        let url = Url::parse("https://www.example.com")?;

        let bm = InsertableItem::Bookmark(InsertableBookmark {
            parent_guid: BookmarkRootGuid::Unfiled.into(),
            position: BookmarkPosition::Append,
            date_added: None,
            last_modified: None,
            guid: None,
            url: url.clone(),
            title: Some("".into()),
        });
        let guid = insert_bookmark(&conn, &bm)?;
        let rb = get_raw_bookmark(&conn, &guid)?.expect("should get the bookmark");
        assert_eq!(rb.title, None);

        let bm2 = InsertableItem::Bookmark(InsertableBookmark {
            parent_guid: BookmarkRootGuid::Unfiled.into(),
            position: BookmarkPosition::Append,
            date_added: None,
            last_modified: None,
            guid: None,
            url: url.clone(),
            title: None,
        });
        let guid2 = insert_bookmark(&conn, &bm2)?;
        let rb2 = get_raw_bookmark(&conn, &guid2)?.expect("should get the bookmark");
        assert_eq!(rb2.title, None);
        Ok(())
    }

    #[test]
    fn test_delete() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = new_mem_connection();

        let guid1 = SyncGuid::new();
        let guid2 = SyncGuid::new();
        let guid2_1 = SyncGuid::new();
        let guid3 = SyncGuid::new();

        let jtree = json!({
            "guid": &BookmarkRootGuid::Unfiled.as_guid(),
            "children": [
                {
                    "guid": &guid1,
                    "title": "the bookmark",
                    "url": "https://www.example.com/"
                },
                {
                    "guid": &guid2,
                    "title": "A folder",
                    "children": [
                        {
                            "guid": &guid2_1,
                            "title": "bookmark in A folder",
                            "url": "https://www.example2.com/"
                        }
                    ]
                },
                {
                    "guid": &guid3,
                    "title": "the last bookmark",
                    "url": "https://www.example3.com/"
                },
            ]
        });

        insert_json_tree(&conn, jtree);

        // Make sure the positions are correct now.
        assert_eq!(get_pos(&conn, &guid1), 0);
        assert_eq!(get_pos(&conn, &guid2), 1);
        assert_eq!(get_pos(&conn, &guid3), 2);

        // Delete the middle folder.
        delete_bookmark(&conn, &guid2)?;
        // Should no longer exist.
        assert!(get_raw_bookmark(&conn, &guid2)?.is_none());
        // Neither should the child.
        assert!(get_raw_bookmark(&conn, &guid2_1)?.is_none());
        // Positions of the remaining should be correct.
        assert_eq!(get_pos(&conn, &guid1), 0);
        assert_eq!(get_pos(&conn, &guid3), 1);

        Ok(())
    }

    #[test]
    fn test_delete_roots() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = new_mem_connection();

        delete_bookmark(&conn, &BookmarkRootGuid::Root.into()).expect_err("can't delete root");
        delete_bookmark(&conn, &BookmarkRootGuid::Unfiled.into())
            .expect_err("can't delete any root");
        Ok(())
    }

    #[test]
    fn test_insert_pos_too_large() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = new_mem_connection();
        let url = Url::parse("https://www.example.com")?;

        let bm = InsertableItem::Bookmark(InsertableBookmark {
            parent_guid: BookmarkRootGuid::Unfiled.into(),
            position: BookmarkPosition::Specific(100),
            date_added: None,
            last_modified: None,
            guid: None,
            url: url.clone(),
            title: Some("the title".into()),
        });
        let guid = insert_bookmark(&conn, &bm)?;

        // re-fetch it.
        let rb = get_raw_bookmark(&conn, &guid)?.expect("should get the bookmark");

        assert_eq!(rb.position, 0, "large value should have been ignored");
        Ok(())
    }

    #[test]
    fn test_update_move_same_parent() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = new_mem_connection();
        let unfiled = &BookmarkRootGuid::Unfiled.as_guid();

        // A helper to make the moves below more concise.
        let do_move = |guid: &str, pos: BookmarkPosition| {
            update_bookmark(
                &conn,
                &guid.into(),
                &UpdatableBookmark {
                    location: UpdateTreeLocation::Position(pos),
                    ..Default::default()
                }
                .into(),
            )
            .expect("update should work");
        };

        // A helper to make the checks below more concise.
        let check_tree = |children: Value| {
            assert_json_tree(
                &conn,
                unfiled,
                json!({
                    "guid": unfiled,
                    "children": children
                }),
            );
        };

        insert_json_tree(
            &conn,
            json!({
                "guid": unfiled,
                "children": [
                    {
                        "guid": "bookmark1___",
                        "url": "https://www.example1.com/"
                    },
                    {
                        "guid": "bookmark2___",
                        "url": "https://www.example2.com/"
                    },
                    {
                        "guid": "bookmark3___",
                        "url": "https://www.example3.com/"
                    },

                ]
            }),
        );

        // Move a bookmark to the end.
        do_move("bookmark2___", BookmarkPosition::Append);
        check_tree(json!([
            {"url": "https://www.example1.com/"},
            {"url": "https://www.example3.com/"},
            {"url": "https://www.example2.com/"},
        ]));

        // Move a bookmark to its existing position
        do_move("bookmark3___", BookmarkPosition::Specific(1));
        check_tree(json!([
            {"url": "https://www.example1.com/"},
            {"url": "https://www.example3.com/"},
            {"url": "https://www.example2.com/"},
        ]));

        // Move a bookmark back 1 position.
        do_move("bookmark2___", BookmarkPosition::Specific(1));
        check_tree(json!([
            {"url": "https://www.example1.com/"},
            {"url": "https://www.example2.com/"},
            {"url": "https://www.example3.com/"},
        ]));

        // Move a bookmark forward 1 position.
        do_move("bookmark2___", BookmarkPosition::Specific(2));
        check_tree(json!([
            {"url": "https://www.example1.com/"},
            {"url": "https://www.example3.com/"},
            {"url": "https://www.example2.com/"},
        ]));

        // Move a bookmark beyond the end.
        do_move("bookmark1___", BookmarkPosition::Specific(10));
        check_tree(json!([
            {"url": "https://www.example3.com/"},
            {"url": "https://www.example2.com/"},
            {"url": "https://www.example1.com/"},
        ]));

        Ok(())
    }

    #[test]
    fn test_update() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = new_mem_connection();
        let unfiled = &BookmarkRootGuid::Unfiled.as_guid();

        insert_json_tree(
            &conn,
            json!({
                "guid": unfiled,
                "children": [
                    {
                        "guid": "bookmark1___",
                        "title": "the bookmark",
                        "url": "https://www.example.com/"
                    },
                    {
                        "guid": "bookmark2___",
                        "title": "another bookmark",
                        "url": "https://www.example2.com/"
                    },
                    {
                        "guid": "folder1_____",
                        "title": "A folder",
                        "children": [
                            {
                                "guid": "bookmark3___",
                                "title": "bookmark in A folder",
                                "url": "https://www.example3.com/"
                            },
                            {
                                "guid": "bookmark4___",
                                "title": "next bookmark in A folder",
                                "url": "https://www.example4.com/"
                            },
                            {
                                "guid": "bookmark5___",
                                "title": "next next bookmark in A folder",
                                "url": "https://www.example5.com/"
                            }
                        ]
                    },
                    {
                        "guid": "bookmark6___",
                        "title": "yet another bookmark",
                        "url": "https://www.example6.com/"
                    },

                ]
            }),
        );

        update_bookmark(
            &conn,
            &"folder1_____".into(),
            &UpdatableFolder {
                title: Some("new name".to_string()),
                ..Default::default()
            }
            .into(),
        )?;
        update_bookmark(
            &conn,
            &"bookmark1___".into(),
            &UpdatableBookmark {
                url: Some(Url::parse("https://www.example3.com/")?),
                title: None,
                ..Default::default()
            }
            .into(),
        )?;

        // A move in the same folder.
        update_bookmark(
            &conn,
            &"bookmark6___".into(),
            &UpdatableBookmark {
                location: UpdateTreeLocation::Position(BookmarkPosition::Specific(2)),
                ..Default::default()
            }
            .into(),
        )?;

        // A move across folders.
        update_bookmark(
            &conn,
            &"bookmark2___".into(),
            &UpdatableBookmark {
                location: UpdateTreeLocation::Parent(
                    "folder1_____".into(),
                    BookmarkPosition::Specific(1),
                ),
                ..Default::default()
            }
            .into(),
        )?;

        assert_json_tree(
            &conn,
            unfiled,
            json!({
                "guid": unfiled,
                "children": [
                    {
                        // We updated the url and title of this.
                        "guid": "bookmark1___",
                        "title": null,
                        "url": "https://www.example3.com/"
                    },
                        // We moved bookmark6 to position=2 (ie, 3rd) of the same
                        // parent, but then moved the existing 2nd item to the
                        // folder, so this ends up second.
                    {
                        "guid": "bookmark6___",
                        "url": "https://www.example6.com/"
                    },
                    {
                        // We changed the name of the folder.
                        "guid": "folder1_____",
                        "title": "new name",
                        "children": [
                            {
                                "guid": "bookmark3___",
                                "url": "https://www.example3.com/"
                            },
                            {
                                // This was moved from the parent to position 1
                                "guid": "bookmark2___",
                                "url": "https://www.example2.com/"
                            },
                            {
                                "guid": "bookmark4___",
                                "url": "https://www.example4.com/"
                            },
                            {
                                "guid": "bookmark5___",
                                "url": "https://www.example5.com/"
                            }
                        ]
                    },

                ]
            }),
        );

        Ok(())
    }

    #[test]
    fn test_update_titles() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = new_mem_connection();
        let guid: SyncGuid = "bookmark1___".into();

        insert_json_tree(
            &conn,
            json!({
                "guid": &BookmarkRootGuid::Unfiled.as_guid(),
                "children": [
                    {
                        "guid": "bookmark1___",
                        "title": "the bookmark",
                        "url": "https://www.example.com/"
                    },
                ],
            }),
        );

        conn.execute("UPDATE moz_bookmarks SET syncChangeCounter = 0", NO_PARAMS)
            .expect("should work");

        // Update of None means no change.
        update_bookmark(
            &conn,
            &guid,
            &UpdatableBookmark {
                title: None,
                ..Default::default()
            }
            .into(),
        )?;
        let bm = get_raw_bookmark(&conn, &guid)?.expect("should exist");
        assert_eq!(bm.title, Some("the bookmark".to_string()));
        assert_eq!(bm.sync_change_counter, 0);

        // Update to the same value is still not a change.
        update_bookmark(
            &conn,
            &guid,
            &UpdatableBookmark {
                title: Some("the bookmark".to_string()),
                ..Default::default()
            }
            .into(),
        )?;
        let bm = get_raw_bookmark(&conn, &guid)?.expect("should exist");
        assert_eq!(bm.title, Some("the bookmark".to_string()));
        assert_eq!(bm.sync_change_counter, 0);

        // Update to an empty string sets it to null
        update_bookmark(
            &conn,
            &guid,
            &UpdatableBookmark {
                title: Some("".to_string()),
                ..Default::default()
            }
            .into(),
        )?;
        let bm = get_raw_bookmark(&conn, &guid)?.expect("should exist");
        assert_eq!(bm.title, None);
        assert_eq!(bm.sync_change_counter, 1);

        Ok(())
    }

    #[test]
    fn test_update_statuses() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = new_mem_connection();
        let unfiled = &BookmarkRootGuid::Unfiled.as_guid();

        let check_change_counters = |guids: Vec<&str>| {
            let sql = "SELECT guid FROM moz_bookmarks WHERE syncChangeCounter != 0";
            let mut stmt = conn.prepare(sql).expect("sql is ok");
            let got_guids: HashSet<String> = stmt
                .query_and_then(NO_PARAMS, |row| -> rusqlite::Result<_> {
                    Ok(row.get_checked::<_, String>(0)?)
                })
                .expect("should work")
                .map(|v| v.unwrap())
                .into_iter()
                .collect();

            assert_eq!(
                got_guids,
                guids.into_iter().map(|v| v.to_string()).collect()
            );
            // reset them all back
            conn.execute("UPDATE moz_bookmarks SET syncChangeCounter = 0", NO_PARAMS)
                .expect("should work");
        };

        let check_last_modified = |guids: Vec<&str>| {
            let sql = "SELECT guid FROM moz_bookmarks
                       WHERE lastModified >= 1000 AND guid != 'root________'";

            let mut stmt = conn.prepare(sql).expect("sql is ok");
            let got_guids: HashSet<String> = stmt
                .query_and_then(NO_PARAMS, |row| -> rusqlite::Result<_> {
                    Ok(row.get_checked::<_, String>(0)?)
                })
                .expect("should work")
                .map(|v| v.unwrap())
                .into_iter()
                .collect();

            assert_eq!(
                got_guids,
                guids.into_iter().map(|v| v.to_string()).collect()
            );
            // reset them all back
            conn.execute("UPDATE moz_bookmarks SET lastModified = 123", NO_PARAMS)
                .expect("should work");
        };

        insert_json_tree(
            &conn,
            json!({
                "guid": unfiled,
                "children": [
                    {
                        "guid": "folder1_____",
                        "title": "A folder",
                        "children": [
                            {
                                "guid": "bookmark1___",
                                "title": "bookmark in A folder",
                                "url": "https://www.example2.com/"
                            },
                            {
                                "guid": "bookmark2___",
                                "title": "next bookmark in A folder",
                                "url": "https://www.example3.com/"
                            },
                        ]
                    },
                    {
                        "guid": "folder2_____",
                        "title": "folder 2",
                    },
                ]
            }),
        );

        // reset all statuses and timestamps.
        conn.execute(
            "UPDATE moz_bookmarks SET syncChangeCounter = 0, lastModified = 123",
            NO_PARAMS,
        )?;

        // update a title - should get a change counter.
        update_bookmark(
            &conn,
            &"bookmark1___".into(),
            &UpdatableBookmark {
                title: Some("new name".to_string()),
                ..Default::default()
            }
            .into(),
        )?;
        check_change_counters(vec!["bookmark1___"]);
        // last modified should be all the way up the tree.
        check_last_modified(vec!["unfiled_____", "folder1_____", "bookmark1___"]);

        // update the position in the same folder.
        update_bookmark(
            &conn,
            &"bookmark1___".into(),
            &UpdatableBookmark {
                location: UpdateTreeLocation::Position(BookmarkPosition::Append),
                ..Default::default()
            }
            .into(),
        )?;
        // parent should be the only thing with a change counter.
        check_change_counters(vec!["folder1_____"]);
        // last modified should be all the way up the tree.
        check_last_modified(vec!["unfiled_____", "folder1_____", "bookmark1___"]);

        // update the position to a different folder.
        update_bookmark(
            &conn,
            &"bookmark1___".into(),
            &UpdatableBookmark {
                location: UpdateTreeLocation::Parent(
                    "folder2_____".into(),
                    BookmarkPosition::Append,
                ),
                ..Default::default()
            }
            .into(),
        )?;
        // Both parents should have a change counter.
        check_change_counters(vec!["folder1_____", "folder2_____"]);
        // last modified should be all the way up the tree and include both parents.
        check_last_modified(vec![
            "unfiled_____",
            "folder1_____",
            "folder2_____",
            "bookmark1___",
        ]);

        Ok(())
    }

    #[test]
    fn test_update_errors() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = new_mem_connection();

        insert_json_tree(
            &conn,
            json!({
                "guid": &BookmarkRootGuid::Unfiled.as_guid(),
                "children": [
                    {
                        "guid": "bookmark1___",
                        "title": "the bookmark",
                        "url": "https://www.example.com/"
                    },
                    {
                        "guid": "folder1_____",
                        "title": "A folder",
                        "children": [
                            {
                                "guid": "bookmark2___",
                                "title": "bookmark in A folder",
                                "url": "https://www.example2.com/"
                            },
                        ]
                    },
                ]
            }),
        );
        // Update an item that doesn't exist.
        update_bookmark(
            &conn,
            &"bookmark9___".into(),
            &UpdatableBookmark {
                ..Default::default()
            }
            .into(),
        )
        .expect_err("should fail to update an item that doesn't exist");

        // A move across to a non-folder
        update_bookmark(
            &conn,
            &"bookmark1___".into(),
            &UpdatableBookmark {
                location: UpdateTreeLocation::Parent(
                    "bookmark2___".into(),
                    BookmarkPosition::Specific(1),
                ),
                ..Default::default()
            }
            .into(),
        )
        .expect_err("can't move to a bookmark");

        // A move to the root
        update_bookmark(
            &conn,
            &"bookmark1___".into(),
            &UpdatableBookmark {
                location: UpdateTreeLocation::Parent(
                    BookmarkRootGuid::Root.as_guid(),
                    BookmarkPosition::Specific(1),
                ),
                ..Default::default()
            }
            .into(),
        )
        .expect_err("can't move to the root");
        Ok(())
    }

    #[test]
    fn test_fetch_root() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = new_mem_connection();

        // Fetch the root
        let t = fetch_tree(&conn, &BookmarkRootGuid::Root.into())?.unwrap();
        let f = match t {
            BookmarkTreeNode::Folder(ref f) => f,
            _ => panic!("tree root must be a folder"),
        };
        assert_eq!(f.guid, Some(BookmarkRootGuid::Root.into()));
        assert_eq!(f.children.len(), 4);
        Ok(())
    }

    #[test]
    fn test_insert_tree() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = new_mem_connection();

        let tree = FolderNode {
            guid: Some(BookmarkRootGuid::Unfiled.into()),
            children: vec![
                BookmarkNode {
                    guid: None,
                    date_added: None,
                    last_modified: None,
                    title: Some("the bookmark".into()),
                    url: Url::parse("https://www.example.com")?,
                }
                .into(),
                FolderNode {
                    title: Some("A folder".into()),
                    children: vec![
                        BookmarkNode {
                            guid: None,
                            date_added: None,
                            last_modified: None,
                            title: Some("bookmark 1 in A folder".into()),
                            url: Url::parse("https://www.example2.com")?,
                        }
                        .into(),
                        BookmarkNode {
                            guid: None,
                            date_added: None,
                            last_modified: None,
                            title: Some("bookmark 2 in A folder".into()),
                            url: Url::parse("https://www.example3.com")?,
                        }
                        .into(),
                    ],
                    ..Default::default()
                }
                .into(),
                BookmarkNode {
                    guid: None,
                    date_added: None,
                    last_modified: None,
                    title: Some("another bookmark".into()),
                    url: Url::parse("https://www.example4.com")?,
                }
                .into(),
            ],
            ..Default::default()
        };
        insert_tree(&conn, &tree)?;

        // check  it.
        assert_json_tree(
            &conn,
            &BookmarkRootGuid::Unfiled.into(),
            json!({
                "guid": &BookmarkRootGuid::Unfiled.as_guid(),
                "children": [
                    {
                        "title": "the bookmark",
                        "url": "https://www.example.com/"
                    },
                    {
                        "title": "A folder",
                        "children": [
                            {
                                "title": "bookmark 1 in A folder",
                                "url": "https://www.example2.com/"
                            },
                            {
                                "title": "bookmark 2 in A folder",
                                "url": "https://www.example3.com/"
                            }
                        ],
                    },
                    {
                        "title": "another bookmark",
                        "url": "https://www.example4.com/",
                    }
                ]
            }),
        );
        Ok(())
    }
}

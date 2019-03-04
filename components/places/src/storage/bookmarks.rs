/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::RowId;
use super::{fetch_page_info, new_page_info};
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

/// Special GUIDs associated with bookmark roots.
/// It's guaranteed that the roots will always have these guids.
#[derive(Debug, PartialEq)]
pub enum BookmarkRootGuid {
    Root,
    Menu,
    Toolbar,
    Unfiled,
    Mobile,
}

impl BookmarkRootGuid {
    pub fn as_guid(&self) -> SyncGuid {
        match self {
            &BookmarkRootGuid::Root => SyncGuid("root________".into()),
            &BookmarkRootGuid::Menu => SyncGuid("menu________".into()),
            &BookmarkRootGuid::Toolbar => SyncGuid("toolbar_____".into()),
            &BookmarkRootGuid::Unfiled => SyncGuid("unfiled_____".into()),
            &BookmarkRootGuid::Mobile => SyncGuid("mobile______".into()),
        }
    }

    pub fn from_guid(guid: &SyncGuid) -> Option<Self> {
        match guid.as_ref() {
            "root________" => Some(BookmarkRootGuid::Root),
            "menu________" => Some(BookmarkRootGuid::Menu),
            "toolbar_____" => Some(BookmarkRootGuid::Toolbar),
            "unfiled_____" => Some(BookmarkRootGuid::Unfiled),
            "mobile______" => Some(BookmarkRootGuid::Mobile),
            _ => None,
        }
    }
}

impl From<BookmarkRootGuid> for SyncGuid {
    fn from(item: BookmarkRootGuid) -> SyncGuid {
        item.as_guid()
    }
}

// Allow comparisons between BookmarkRootGuid and SyncGuids
impl PartialEq<BookmarkRootGuid> for SyncGuid {
    fn eq(&self, other: &BookmarkRootGuid) -> bool {
        *self == other.as_guid()
    }
}

impl PartialEq<SyncGuid> for BookmarkRootGuid {
    fn eq(&self, other: &SyncGuid) -> bool {
        self.as_guid() == *other
    }
}

fn create_root(
    db: &Connection,
    title: &str,
    guid: &SyncGuid,
    position: u32,
    when: &Timestamp,
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
        (":date_added", when),
        (":last_modified", when),
        (":guid", guid),
        (":sync_status", &SyncStatus::New),
    ];
    db.execute_named_cached(&sql, &params)?;
    Ok(())
}

pub fn create_bookmark_roots(db: &Connection) -> Result<()> {
    let now = Timestamp::now();
    create_root(db, "root", &BookmarkRootGuid::Root.into(), 0, &now)?;
    create_root(db, "menu", &BookmarkRootGuid::Menu.into(), 0, &now)?;
    create_root(db, "toolbar", &BookmarkRootGuid::Toolbar.into(), 1, &now)?;
    create_root(db, "unfiled", &BookmarkRootGuid::Unfiled.into(), 2, &now)?;
    create_root(db, "mobile", &BookmarkRootGuid::Mobile.into(), 3, &now)?;
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
    db: &impl ConnExt,
    pos: &BookmarkPosition,
    parent_id: RowId,
    cur_child_count: u32,
) -> Result<u32> {
    Ok(match pos {
        BookmarkPosition::Specific(specified) => {
            let actual = min(*specified, cur_child_count);
            // must reorder existing children.
            db.execute_named_cached(
                "UPDATE moz_bookmarks SET position = position + 1
                 WHERE parent = :parent_id
                 AND position >= :position",
                &[(":parent_id", &parent_id), (":position", &actual)],
            )?;
            actual
        }
        BookmarkPosition::Append => cur_child_count,
    })
}

/// Updates the position of existing items so that the deletion of a child
/// from the position specified leaves all siblings with the correct position.
fn update_pos_for_deletion(db: &impl ConnExt, pos: u32, parent_id: RowId) -> Result<()> {
    db.execute_named_cached(
        "UPDATE moz_bookmarks SET position = position - 1
         WHERE parent = :parent
         AND position >= :position",
        &[(":parent", &parent_id), (":position", &pos)],
    )?;
    Ok(())
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

pub fn insert_bookmark(db: &impl ConnExt, bm: &InsertableItem) -> Result<SyncGuid> {
    let tx = db.unchecked_transaction()?;
    let result = insert_bookmark_in_tx(db, bm);
    super::delete_pending_temp_tables(db.conn())?;
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

fn insert_bookmark_in_tx(db: &impl ConnExt, bm: &InsertableItem) -> Result<SyncGuid> {
    // find the row ID of the parent.
    if BookmarkRootGuid::from_guid(&bm.parent_guid()) == Some(BookmarkRootGuid::Root) {
        return Err(InvalidPlaceInfo::InvalidGuid.into());
    }
    let parent_guid = bm.parent_guid();
    let parent = get_raw_bookmark(db, parent_guid)?
        .ok_or_else(|| InvalidPlaceInfo::InvalidParent(parent_guid.to_string()))?;
    if parent.bookmark_type != BookmarkType::Folder {
        return Err(InvalidPlaceInfo::InvalidParent(parent_guid.to_string()).into());
    }
    // Do the "position" dance.
    let position = resolve_pos_for_insert(db, bm.position(), parent.row_id, parent.child_count)?;

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

    let guid = bm.guid().clone().unwrap_or_else(|| SyncGuid::new());
    let date_added = bm.date_added().unwrap_or_else(|| Timestamp::now());
    // last_modified can't be before date_added
    let last_modified = max(
        bm.last_modified().unwrap_or_else(|| Timestamp::now()),
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
    Ok(guid)
}

/// Delete the specified bookmark. Returns true if a bookmark with the guid
/// existed and was deleted, false otherwise.
pub fn delete_bookmark(db: &impl ConnExt, guid: &SyncGuid) -> Result<bool> {
    let tx = db.unchecked_transaction()?;
    let result = delete_bookmark_in_tx(db, guid);
    match result {
        Ok(_) => tx.commit()?,
        Err(_) => tx.rollback()?,
    }
    result
}

fn delete_bookmark_in_tx(db: &impl ConnExt, guid: &SyncGuid) -> Result<bool> {
    // Can't delete a root.
    if BookmarkRootGuid::from_guid(guid).is_some() {
        return Err(InvalidPlaceInfo::InvalidGuid.into());
    }
    let record = match get_raw_bookmark(db, guid)? {
        Some(r) => r,
        None => {
            log::debug!("Can't delete bookmark '{:?}' as it doesn't exist", guid);
            return Ok(false);
        }
    };
    // must reorder existing children.
    update_pos_for_deletion(db, record.position, record.parent_id)?;
    // and delete - children are recursively deleted.
    db.execute_named_cached(
        "DELETE from moz_bookmarks WHERE id = :id",
        &[(":id", &record.row_id)],
    )?;
    super::delete_pending_temp_tables(db.conn())?;
    Ok(true)
}

/// Support for modifying bookmarks, including changing the location in
/// the tree.

/// Used instead of Option<String> for updating the title, so we can
/// differentiate between "no change", "set to null" and "set to a value"
/// Could trivially use <T>, but title is the only use-case for now, so it's
/// a little clearer to leave it specific.
#[derive(Debug, Clone)]
pub enum UpdateTitle {
    None,         // no change.
    Null,         // change the existing value to null.
    Some(String), // change the existing value to this
}

impl Default for UpdateTitle {
    fn default() -> Self {
        UpdateTitle::None
    }
}

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
    pub title: UpdateTitle,
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
    // There's no good reason to differentiate `null` from `""` in a folder,
    // but for consistency we allow it.
    pub title: UpdateTitle,
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

pub fn update_bookmark(db: &impl ConnExt, guid: &SyncGuid, item: &UpdatableItem) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    let result = update_bookmark_in_tx(db, guid, item);
    match result {
        Ok(_) => tx.commit()?,
        Err(_) => tx.rollback()?,
    }
    result
}

fn update_bookmark_in_tx(db: &impl ConnExt, guid: &SyncGuid, item: &UpdatableItem) -> Result<()> {
    let existing =
        get_raw_bookmark(db, guid)?.ok_or_else(|| InvalidPlaceInfo::NoItem(guid.to_string()))?;
    if existing.bookmark_type != item.bookmark_type() {
        return Err(InvalidPlaceInfo::InvalidBookmarkType.into());
    }

    let update_old_parent_status;
    let update_new_parent_status;
    // to make our life easier we update every field, using existing when
    // no value is specified.
    let parent_id;
    let position;
    match item.location() {
        UpdateTreeLocation::None => {
            parent_id = existing.parent_id;
            position = existing.position;
            update_old_parent_status = false;
            update_new_parent_status = false;
        }
        UpdateTreeLocation::Position(pos) => {
            parent_id = existing.parent_id;
            update_old_parent_status = true;
            update_new_parent_status = false;
            // Not clear that InvalidParent is the correct error here - probably
            // should be a "stuff is corrupt" error? Or maybe we should fix it
            // here by reparenting to unfiled?
            let parent = get_raw_bookmark(db, &existing.parent_guid)?
                .ok_or_else(|| InvalidPlaceInfo::InvalidParent(existing.parent_guid.to_string()))?;
            update_pos_for_deletion(db, existing.position, parent.row_id)?;
            // We just removed a child, so actual child count is now parent.child_count - 1
            position = resolve_pos_for_insert(db, pos, parent.row_id, parent.child_count - 1)?;
        }
        UpdateTreeLocation::Parent(new_parent_guid, pos) => {
            if BookmarkRootGuid::from_guid(&new_parent_guid) == Some(BookmarkRootGuid::Root) {
                return Err(InvalidPlaceInfo::InvalidGuid.into());
            }
            let new_parent = get_raw_bookmark(db, &new_parent_guid)?
                .ok_or_else(|| InvalidPlaceInfo::InvalidParent(new_parent_guid.to_string()))?;
            if new_parent.bookmark_type != BookmarkType::Folder {
                return Err(InvalidPlaceInfo::InvalidParent(new_parent_guid.to_string()).into());
            }
            parent_id = new_parent.row_id;
            update_old_parent_status = true;
            update_new_parent_status = true;
            // This position change is more complicated across parents.
            // As above, this failure really means "stuff is corrupt" and we
            // could consider fixing the tree instead of throwing?
            let existing_parent = get_raw_bookmark(db, &existing.parent_guid)?
                .ok_or_else(|| InvalidPlaceInfo::InvalidParent(existing.parent_guid.to_string()))?;
            update_pos_for_deletion(db, existing.position, existing_parent.row_id)?;
            position = resolve_pos_for_insert(db, pos, new_parent.row_id, new_parent.child_count)?;
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
    let update_title = match item {
        UpdatableItem::Bookmark(b) => &b.title,
        UpdatableItem::Folder(f) => &f.title,
        UpdatableItem::Separator(_) => &UpdateTitle::None,
    };
    let title: Option<String> = match update_title {
        UpdateTitle::None => existing.title,
        UpdateTitle::Null => None,
        UpdateTitle::Some(t) => Some(t.clone()),
    };

    let now = Timestamp::now();

    // The change counter for this item is only updated if the item has
    // been synced.
    let sync_change_counter = if existing.sync_status == SyncStatus::Normal {
        existing.sync_change_counter + 1
    } else {
        existing.sync_change_counter
    };

    let sql = "
        UPDATE moz_bookmarks SET
            fk = :fk,
            parent = :parent,
            position = :position,
            title = :title,
            lastModified = :now,
            syncChangeCounter = :sync_change_counter
        WHERE id = :id";

    db.execute_named_cached(
        sql,
        &[
            (":fk", &place_id),
            (":parent", &parent_id),
            (":position", &position),
            (":title", &title),
            (":now", &now),
            (":sync_change_counter", &sync_change_counter),
            (":id", &existing.row_id),
        ],
    )?;

    let sql_counter = "
        UPDATE moz_bookmarks SET syncChangeCounter = syncChangeCounter + 1
        WHERE id = :parent_id AND syncStatus = :status_normal";

    if update_old_parent_status {
        db.execute_named_cached(
            sql_counter,
            &[
                (":parent_id", &existing.parent_id),
                (":status_normal", &(SyncStatus::Normal as u8)),
            ],
        )?;
    }
    if update_new_parent_status {
        db.execute_named_cached(
            sql_counter,
            &[
                (":parent_id", &parent_id),
                (":status_normal", &(SyncStatus::Normal as u8)),
            ],
        )?;
    }
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
                let my_guid = f.guid.clone().unwrap_or_else(|| SyncGuid::new());
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

pub fn insert_tree(db: &impl ConnExt, tree: &FolderNode) -> Result<()> {
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
    super::delete_pending_temp_tables(db.conn())?;
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
pub fn fetch_tree(db: &impl ConnExt, item_guid: &SyncGuid) -> Result<Option<BookmarkTreeNode>> {
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
               d.position, IFNULL(d.title, "") AS title, d.dateAdded,
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

    // For all remaining rows, build a pseudo-tree that maps parent GUIDs to
    // ordered children. We need this intermediate step because SQLite returns
    // results in level order, so we'll see a node's siblings and cousins (same
    // level, but different parents) before any of their descendants.
    let mut pseudo_tree: HashMap<SyncGuid, Vec<BookmarkTreeNode>> = HashMap::new();
    for result in results {
        let row = result?;
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
    parent_id: RowId,
    parent_guid: SyncGuid,
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

fn get_raw_bookmark(db: &impl ConnExt, guid: &SyncGuid) -> Result<Option<RawBookmark>> {
    // sql is based on fetchBookmark() in Desktop's Bookmarks.jsm, with 'fk' added.
    Ok(db.try_query_row(
        "
        SELECT b.guid, p.guid AS parentGuid, b.position,
               b.dateAdded, b.lastModified, b.type, b.title AS title,
               h.url AS url, b.id AS _id, b.parent AS _parentId,
               (SELECT count(*) FROM moz_bookmarks WHERE parent = b.id) AS _childCount,
               p.parent AS _grandParentId, b.syncStatus AS _syncStatus,
               -- the columns below don't appear in the desktop query
               b.fk, b.syncChangeCounter
       FROM moz_bookmarks b
       LEFT JOIN moz_bookmarks p ON p.id = b.parent
       LEFT JOIN moz_places h ON h.id = b.fk
       WHERE b.guid = :guid",
        &[(":guid", guid)],
        RawBookmark::from_row,
        true,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::PlacesDb;
    use serde_json::Value;

    fn insert_json_tree(conn: &PlacesDb, jtree: Value) {
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
    fn check_positions(conn: &PlacesDb, folder: &SyncGuid) {
        let sql = "
            SELECT position FROM moz_bookmarks
            WHERE parent = (SELECT id from moz_bookmarks WHERE guid = :folder_guid)
            ORDER BY position
        ";

        let mut stmt = conn.prepare(sql).expect("sql is ok");
        let positions: Vec<usize> = stmt
            .query_and_then_named(&[(":folder_guid", folder)], |row| -> rusqlite::Result<_> {
                Ok(row.get_checked::<_, u32>(0)?)
            })
            .expect("should work")
            .map(|v| v.unwrap() as usize)
            .collect();

        // checking things this way gives nice output when it fails.
        let expected: Vec<usize> = (0..positions.len()).collect();
        assert_eq!(positions, expected);
    }

    #[test]
    fn test_insert() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = PlacesDb::open_in_memory(None)?;
        let url = Url::parse("https://www.example.com")?;

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
        assert_eq!(rb.parent_guid, BookmarkRootGuid::Unfiled.as_guid());
        assert_eq!(rb.position, 0);
        assert_eq!(rb.title, Some("the title".into()));
        assert_eq!(rb.url, Some(url));
        assert_eq!(rb.sync_status, SyncStatus::New);
        assert_eq!(rb.sync_change_counter, 1);
        assert_eq!(rb.child_count, 0);
        Ok(())
    }

    #[test]
    fn test_delete() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = PlacesDb::open_in_memory(None)?;

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
        let conn = PlacesDb::open_in_memory(None)?;

        delete_bookmark(&conn, &BookmarkRootGuid::Root.into()).expect_err("can't delete root");
        delete_bookmark(&conn, &BookmarkRootGuid::Unfiled.into())
            .expect_err("can't delete any root");
        Ok(())
    }

    #[test]
    fn test_insert_pos_too_large() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = PlacesDb::open_in_memory(None)?;
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
        let conn = PlacesDb::open_in_memory(None)?;
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
            check_positions(&conn, unfiled);
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

        Ok(())
    }

    #[test]
    fn test_update() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = PlacesDb::open_in_memory(None)?;
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
                title: UpdateTitle::Some("new name".to_string()),
                ..Default::default()
            }
            .into(),
        )?;
        update_bookmark(
            &conn,
            &"bookmark1___".into(),
            &UpdatableBookmark {
                url: Some(Url::parse("https://www.example3.com/")?),
                title: UpdateTitle::Null,
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

        // explicitly check positions to ensure no holes or dupes.
        check_positions(&conn, unfiled);
        check_positions(&conn, &"folder1_____".into());

        Ok(())
    }

    #[test]
    fn test_update_errors() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = PlacesDb::open_in_memory(None)?;

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
        let conn = PlacesDb::open_in_memory(None)?;

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
        let conn = PlacesDb::open_in_memory(None)?;

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

        // re-fetch it.
        let fetched = fetch_tree(&conn, &BookmarkRootGuid::Unfiled.into())?.unwrap();

        let expected = json!({
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
        });
        let deser_tree: BookmarkTreeNode = serde_json::from_value(expected)?;
        assert_eq!(fetched, deser_tree);
        Ok(())
    }
}

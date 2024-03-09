/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::RowId;
use super::{delete_meta, put_meta};
use super::{fetch_page_info, new_page_info};
use crate::bookmark_sync::engine::{
    COLLECTION_SYNCID_META_KEY, GLOBAL_SYNCID_META_KEY, LAST_SYNC_META_KEY,
};
use crate::db::PlacesDb;
use crate::error::*;
use crate::types::{BookmarkType, SyncStatus};
use rusqlite::{self, Connection, Row};
#[cfg(test)]
use serde_json::{self, json};
use sql_support::{self, repeat_sql_vars, ConnExt};
use std::cmp::{max, min};
use sync15::engine::EngineSyncAssociation;
use sync_guid::Guid as SyncGuid;
use types::Timestamp;
use url::Url;

pub use root_guid::{BookmarkRootGuid, USER_CONTENT_ROOTS};

mod conversions;
pub mod fetch;
pub mod json_tree;
mod root_guid;

fn create_root(
    db: &Connection,
    title: &str,
    guid: &SyncGuid,
    position: u32,
    when: Timestamp,
) -> rusqlite::Result<()> {
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
        BookmarkRootGuid::Root.as_guid().as_str()
    );
    let params = rusqlite::named_params! {
        ":item_type": &BookmarkType::Folder,
        ":item_position": &position,
        ":item_title": &title,
        ":date_added": &when,
        ":last_modified": &when,
        ":guid": guid,
        ":sync_status": &SyncStatus::New,
    };
    db.execute_cached(&sql, params)?;
    Ok(())
}

pub fn create_bookmark_roots(db: &Connection) -> rusqlite::Result<()> {
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
    Specific { pos: u32 },
    Append,
}

/// Helpers to deal with managing the position correctly.
///
/// Updates the position of existing items so that the insertion of a child in
/// the position specified leaves all siblings with the correct position.
/// Returns the index the item should be inserted at.
fn resolve_pos_for_insert(
    db: &PlacesDb,
    pos: BookmarkPosition,
    parent: &RawBookmark,
) -> Result<u32> {
    Ok(match pos {
        BookmarkPosition::Specific { pos } => {
            let actual = min(pos, parent.child_count);
            // must reorder existing children.
            db.execute_cached(
                "UPDATE moz_bookmarks SET position = position + 1
                 WHERE parent = :parent_id
                 AND position >= :position",
                &[
                    (":parent_id", &parent.row_id as &dyn rusqlite::ToSql),
                    (":position", &actual),
                ],
            )?;
            actual
        }
        BookmarkPosition::Append => parent.child_count,
    })
}

/// Updates the position of existing items so that the deletion of a child
/// from the position specified leaves all siblings with the correct position.
fn update_pos_for_deletion(db: &PlacesDb, pos: u32, parent_id: RowId) -> Result<()> {
    db.execute_cached(
        "UPDATE moz_bookmarks SET position = position - 1
         WHERE parent = :parent
         AND position >= :position",
        &[
            (":parent", &parent_id as &dyn rusqlite::ToSql),
            (":position", &pos),
        ],
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
        BookmarkPosition::Specific { pos } => min(pos, parent.child_count - 1),
        BookmarkPosition::Append => parent.child_count - 1,
    };
    db.execute_cached(
        "UPDATE moz_bookmarks
         SET position = CASE WHEN :new_index < :cur_index
            THEN position + 1
            ELSE position - 1
         END
         WHERE parent = :parent_id
         AND position BETWEEN :low_index AND :high_index",
        &[
            (":new_index", &new_index as &dyn rusqlite::ToSql),
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
    fn from(b: InsertableBookmark) -> Self {
        InsertableItem::Bookmark { b }
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
    fn from(s: InsertableSeparator) -> Self {
        InsertableItem::Separator { s }
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
    pub children: Vec<InsertableItem>,
}

impl From<InsertableFolder> for InsertableItem {
    fn from(f: InsertableFolder) -> Self {
        InsertableItem::Folder { f }
    }
}

// The type used to insert the actual item.
#[derive(Debug, Clone)]
pub enum InsertableItem {
    Bookmark { b: InsertableBookmark },
    Separator { s: InsertableSeparator },
    Folder { f: InsertableFolder },
}

// We allow all "common" fields from the sub-types to be getters on the
// InsertableItem type.
macro_rules! impl_common_bookmark_getter {
    ($getter_name:ident, $T:ty) => {
        fn $getter_name(&self) -> &$T {
            match self {
                InsertableItem::Bookmark { b } => &b.$getter_name,
                InsertableItem::Separator { s } => &s.$getter_name,
                InsertableItem::Folder { f } => &f.$getter_name,
            }
        }
    };
}

impl InsertableItem {
    fn bookmark_type(&self) -> BookmarkType {
        match self {
            InsertableItem::Bookmark { .. } => BookmarkType::Bookmark,
            InsertableItem::Separator { .. } => BookmarkType::Separator,
            InsertableItem::Folder { .. } => BookmarkType::Folder,
        }
    }
    impl_common_bookmark_getter!(parent_guid, SyncGuid);
    impl_common_bookmark_getter!(position, BookmarkPosition);
    impl_common_bookmark_getter!(date_added, Option<Timestamp>);
    impl_common_bookmark_getter!(last_modified, Option<Timestamp>);
    impl_common_bookmark_getter!(guid, Option<SyncGuid>);

    // We allow a setter for parent_guid and timestamps to help when inserting a tree.
    fn set_parent_guid(&mut self, guid: SyncGuid) {
        match self {
            InsertableItem::Bookmark { b } => b.parent_guid = guid,
            InsertableItem::Separator { s } => s.parent_guid = guid,
            InsertableItem::Folder { f } => f.parent_guid = guid,
        }
    }

    fn set_last_modified(&mut self, ts: Timestamp) {
        match self {
            InsertableItem::Bookmark { b } => b.last_modified = Some(ts),
            InsertableItem::Separator { s } => s.last_modified = Some(ts),
            InsertableItem::Folder { f } => f.last_modified = Some(ts),
        }
    }

    fn set_date_added(&mut self, ts: Timestamp) {
        match self {
            InsertableItem::Bookmark { b } => b.date_added = Some(ts),
            InsertableItem::Separator { s } => s.date_added = Some(ts),
            InsertableItem::Folder { f } => f.date_added = Some(ts),
        }
    }
}

pub fn insert_bookmark(db: &PlacesDb, bm: InsertableItem) -> Result<SyncGuid> {
    let tx = db.begin_transaction()?;
    let result = insert_bookmark_in_tx(db, bm);
    super::delete_pending_temp_tables(db)?;
    match result {
        Ok(_) => tx.commit()?,
        Err(_) => tx.rollback()?,
    }
    result
}

pub fn maybe_truncate_title<'a>(t: &Option<&'a str>) -> Option<&'a str> {
    use super::TITLE_LENGTH_MAX;
    use crate::util::slice_up_to;
    t.map(|title| slice_up_to(title, TITLE_LENGTH_MAX))
}

fn insert_bookmark_in_tx(db: &PlacesDb, bm: InsertableItem) -> Result<SyncGuid> {
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
        InsertableItem::Bookmark { ref b } => {
            let page_info = match fetch_page_info(db, &b.url)? {
                Some(info) => info.page,
                None => new_page_info(db, &b.url, None)?,
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

    let guid = bm.guid().clone().unwrap_or_else(SyncGuid::random);
    if !guid.is_valid_for_places() || !guid.is_valid_for_sync_server() {
        return Err(InvalidPlaceInfo::InvalidGuid.into());
    }
    let date_added = bm.date_added().unwrap_or_else(Timestamp::now);
    // last_modified can't be before date_added
    let last_modified = max(
        bm.last_modified().unwrap_or_else(Timestamp::now),
        date_added,
    );

    let bookmark_type = bm.bookmark_type();
    match bm {
        InsertableItem::Bookmark { ref b } => {
            let title = maybe_truncate_title(&b.title.as_deref());
            db.execute_cached(
                sql,
                &[
                    (":fk", &fk as &dyn rusqlite::ToSql),
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
        InsertableItem::Separator { .. } => {
            db.execute_cached(
                sql,
                &[
                    (":type", &bookmark_type as &dyn rusqlite::ToSql),
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
        InsertableItem::Folder { f } => {
            let title = maybe_truncate_title(&f.title.as_deref());
            db.execute_cached(
                sql,
                &[
                    (":type", &bookmark_type as &dyn rusqlite::ToSql),
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
            // now recurse for children
            for mut child in f.children.into_iter() {
                // As a special case for trees, each child in a folder can specify
                // Guid::Empty as the parent.
                let specified_parent_guid = child.parent_guid();
                if specified_parent_guid.is_empty() {
                    child.set_parent_guid(guid.clone());
                } else if *specified_parent_guid != guid {
                    return Err(
                        InvalidPlaceInfo::InvalidParent(specified_parent_guid.to_string()).into(),
                    );
                }
                // If children have defaults for last_modified and date_added we use the parent
                if child.last_modified().is_none() {
                    child.set_last_modified(last_modified);
                }
                if child.date_added().is_none() {
                    child.set_date_added(date_added);
                }
                insert_bookmark_in_tx(db, child)?;
            }
        }
    };

    // Bump the parent's change counter.
    let sql_counter = "
        UPDATE moz_bookmarks SET syncChangeCounter = syncChangeCounter + 1
        WHERE id = :parent_id";
    db.execute_cached(sql_counter, &[(":parent_id", &parent.row_id)])?;

    Ok(guid)
}

/// Delete the specified bookmark. Returns true if a bookmark with the guid
/// existed and was deleted, false otherwise.
pub fn delete_bookmark(db: &PlacesDb, guid: &SyncGuid) -> Result<bool> {
    let tx = db.begin_transaction()?;
    let result = delete_bookmark_in_tx(db, guid);
    match result {
        Ok(_) => tx.commit()?,
        Err(_) => tx.rollback()?,
    }
    result
}

fn delete_bookmark_in_tx(db: &PlacesDb, guid: &SyncGuid) -> Result<bool> {
    // Can't delete a root.
    if let Some(root) = BookmarkRootGuid::well_known(guid.as_str()) {
        return Err(InvalidPlaceInfo::CannotUpdateRoot(root).into());
    }
    let record = match get_raw_bookmark(db, guid)? {
        Some(r) => r,
        None => {
            debug!("Can't delete bookmark '{:?}' as it doesn't exist", guid);
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
    db.execute_cached(
        "DELETE from moz_bookmarks WHERE id = :id",
        &[(":id", &record.row_id)],
    )?;
    super::delete_pending_temp_tables(db)?;
    Ok(true)
}

/// Support for modifying bookmarks, including changing the location in
/// the tree.

// Used to specify how the location of the item in the tree should be updated.
#[derive(Debug, Default, Clone)]
pub enum UpdateTreeLocation {
    #[default]
    None, // no change
    Position {
        pos: BookmarkPosition,
    }, // new position in the same folder.
    Parent {
        guid: SyncGuid,
        pos: BookmarkPosition,
    }, // new parent
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
    fn from(b: UpdatableBookmark) -> Self {
        UpdatableItem::Bookmark { b }
    }
}

#[derive(Debug, Clone)]
pub struct UpdatableSeparator {
    pub location: UpdateTreeLocation,
}

impl From<UpdatableSeparator> for UpdatableItem {
    fn from(s: UpdatableSeparator) -> Self {
        UpdatableItem::Separator { s }
    }
}

#[derive(Debug, Clone, Default)]
pub struct UpdatableFolder {
    pub location: UpdateTreeLocation,
    pub title: Option<String>,
}

impl From<UpdatableFolder> for UpdatableItem {
    fn from(f: UpdatableFolder) -> Self {
        UpdatableItem::Folder { f }
    }
}

// The type used to update the actual item.
#[derive(Debug, Clone)]
pub enum UpdatableItem {
    Bookmark { b: UpdatableBookmark },
    Separator { s: UpdatableSeparator },
    Folder { f: UpdatableFolder },
}

impl UpdatableItem {
    fn bookmark_type(&self) -> BookmarkType {
        match self {
            UpdatableItem::Bookmark { .. } => BookmarkType::Bookmark,
            UpdatableItem::Separator { .. } => BookmarkType::Separator,
            UpdatableItem::Folder { .. } => BookmarkType::Folder,
        }
    }

    pub fn location(&self) -> &UpdateTreeLocation {
        match self {
            UpdatableItem::Bookmark { b } => &b.location,
            UpdatableItem::Separator { s } => &s.location,
            UpdatableItem::Folder { f } => &f.location,
        }
    }
}

/// We don't require bookmark type for updates on the other side of the FFI,
/// since the type is immutable, and iOS wants to be able to move bookmarks by
/// GUID. We also don't/can't enforce as much in the Kotlin/Swift type system
/// as we can/do in Rust.
///
/// This is a type that represents the data we get from the FFI, which we then
/// turn into a `UpdatableItem` that we can actually use (we do this by
/// reading the type out of the DB, but we can do that transactionally, so it's
/// not a problem).
///
/// It's basically an intermediate between the protobuf message format and
/// `UpdatableItem`, used to avoid needing to pass in the `type` to update, and
/// to give us a place to check things that we can't enforce in Swift/Kotlin's
/// type system, but that we do in Rust's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookmarkUpdateInfo {
    pub guid: SyncGuid,
    pub title: Option<String>,
    pub url: Option<String>,
    pub parent_guid: Option<SyncGuid>,
    pub position: Option<u32>,
}

pub fn update_bookmark_from_info(db: &PlacesDb, info: BookmarkUpdateInfo) -> Result<()> {
    let tx = db.begin_transaction()?;
    let existing = get_raw_bookmark(db, &info.guid)?
        .ok_or_else(|| InvalidPlaceInfo::NoSuchGuid(info.guid.to_string()))?;
    let (guid, updatable) = info.into_updatable(existing.bookmark_type)?;

    update_bookmark_in_tx(db, &guid, &updatable, existing)?;
    tx.commit()?;
    Ok(())
}

pub fn update_bookmark(db: &PlacesDb, guid: &SyncGuid, item: &UpdatableItem) -> Result<()> {
    let tx = db.begin_transaction()?;
    let existing = get_raw_bookmark(db, guid)?
        .ok_or_else(|| InvalidPlaceInfo::NoSuchGuid(guid.to_string()))?;
    let result = update_bookmark_in_tx(db, guid, item, existing);
    super::delete_pending_temp_tables(db)?;
    // Note: `tx` automatically rolls back on drop if we don't commit
    tx.commit()?;
    result
}

fn update_bookmark_in_tx(
    db: &PlacesDb,
    guid: &SyncGuid,
    item: &UpdatableItem,
    raw: RawBookmark,
) -> Result<()> {
    // if guid is root
    if BookmarkRootGuid::well_known(guid.as_str()).is_some() {
        return Err(InvalidPlaceInfo::CannotUpdateRoot(BookmarkRootGuid::Root).into());
    }
    let existing_parent_guid = raw
        .parent_guid
        .as_ref()
        .ok_or_else(|| Corruption::NonRootWithoutParent(guid.to_string()))?;

    let existing_parent_id = raw
        .parent_id
        .ok_or_else(|| Corruption::NoParent(guid.to_string(), existing_parent_guid.to_string()))?;

    if raw.bookmark_type != item.bookmark_type() {
        return Err(InvalidPlaceInfo::MismatchedBookmarkType(
            raw.bookmark_type as u8,
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
            position = raw.position;
            update_old_parent_status = false;
            update_new_parent_status = false;
        }
        UpdateTreeLocation::Position { pos } => {
            parent_id = existing_parent_id;
            update_old_parent_status = true;
            update_new_parent_status = false;
            let parent = get_raw_bookmark(db, existing_parent_guid)?.ok_or_else(|| {
                Corruption::NoParent(guid.to_string(), existing_parent_guid.to_string())
            })?;
            position = update_pos_for_move(db, *pos, &raw, &parent)?;
        }
        UpdateTreeLocation::Parent {
            guid: new_parent_guid,
            pos,
        } => {
            if new_parent_guid == BookmarkRootGuid::Root {
                return Err(InvalidPlaceInfo::CannotUpdateRoot(BookmarkRootGuid::Root).into());
            }
            let new_parent = get_raw_bookmark(db, new_parent_guid)?
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
            update_pos_for_deletion(db, raw.position, existing_parent.row_id)?;
            position = resolve_pos_for_insert(db, *pos, &new_parent)?;
        }
    };
    let place_id = match item {
        UpdatableItem::Bookmark { b } => match &b.url {
            None => raw.place_id,
            Some(url) => {
                let page_info = match fetch_page_info(db, url)? {
                    Some(info) => info.page,
                    None => new_page_info(db, url, None)?,
                };
                Some(page_info.row_id)
            }
        },
        _ => {
            // Updating a non-bookmark item, so the existing item must not
            // have a place_id
            assert_eq!(raw.place_id, None);
            None
        }
    };
    // While we could let the SQL take care of being clever about the update
    // via, say `title = NULLIF(IFNULL(:title, title), '')`, this code needs
    // to know if it changed so the sync counter can be managed correctly.
    let update_title = match item {
        UpdatableItem::Bookmark { b } => &b.title,
        UpdatableItem::Folder { f } => &f.title,
        UpdatableItem::Separator { .. } => &None,
    };

    let title: Option<String> = match update_title {
        None => raw.title.clone(),
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

    let change_incr = title != raw.title || place_id != raw.place_id;

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

    db.execute_cached(
        sql,
        &[
            (":fk", &place_id as &dyn rusqlite::ToSql),
            (":parent", &parent_id),
            (":position", &position),
            (":title", &maybe_truncate_title(&title.as_deref())),
            (":now", &now),
            (":change_incr", &(change_incr as u32)),
            (":id", &raw.row_id),
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
        db.execute_cached(sql_counter, &[(":parent_id", &existing_parent_id)])?;
    }
    if update_new_parent_status {
        set_ancestors_last_modified(db, parent_id, now)?;
        db.execute_cached(sql_counter, &[(":parent_id", &parent_id)])?;
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
    db.execute_cached(
        sql,
        &[
            (":parent_id", &parent_id as &dyn rusqlite::ToSql),
            (":type", &(BookmarkType::Folder as u8)),
            (":time", &time),
        ],
    )?;
    Ok(())
}

/// Get the URL of the bookmark matching a keyword
pub fn bookmarks_get_url_for_keyword(db: &PlacesDb, keyword: &str) -> Result<Option<Url>> {
    let bookmark_url = db.try_query_row(
        "SELECT h.url FROM moz_keywords k
         JOIN moz_places h ON h.id = k.place_id
         WHERE k.keyword = :keyword",
        &[(":keyword", &keyword)],
        |row| row.get::<_, String>("url"),
        true,
    )?;

    match bookmark_url {
        Some(b) => match Url::parse(&b) {
            Ok(u) => Ok(Some(u)),
            Err(e) => {
                // We don't have the guid to log and the keyword is PII...
                warn!("ignoring invalid url: {:?}", e);
                Ok(None)
            }
        },
        None => Ok(None),
    }
}

// Counts the number of bookmark items in the bookmark trees under the specified GUIDs.
// Does not count folder items, separators. A set of empty folders will return zero, as will
// a set of non-existing GUIDs or guids of a non-folder item.
// The result is implementation dependant if the trees overlap.
pub fn count_bookmarks_in_trees(db: &PlacesDb, item_guids: &[SyncGuid]) -> Result<u32> {
    if item_guids.is_empty() {
        return Ok(0);
    }
    let vars = repeat_sql_vars(item_guids.len());
    let sql = format!(
        r#"
        WITH RECURSIVE bookmark_tree(id, parent, type)
        AS (
            SELECT id, parent, type FROM moz_bookmarks
            WHERE parent IN (SELECT id from moz_bookmarks WHERE guid IN ({vars}))
            UNION ALL
            SELECT bm.id, bm.parent, bm.type FROM moz_bookmarks bm, bookmark_tree bt WHERE bm.parent = bt.id
        )
    SELECT COUNT(*) from bookmark_tree WHERE type = {};
    "#,
        BookmarkType::Bookmark as u8
    );
    let params = rusqlite::params_from_iter(item_guids);
    Ok(db.try_query_one(&sql, params, true)?.unwrap_or_default())
}

/// Erases all bookmarks and resets all Sync metadata.
pub fn delete_everything(db: &PlacesDb) -> Result<()> {
    let tx = db.begin_transaction()?;
    db.execute_batch(&format!(
        "DELETE FROM moz_bookmarks
         WHERE guid NOT IN ('{}', '{}', '{}', '{}', '{}');",
        BookmarkRootGuid::Root.as_str(),
        BookmarkRootGuid::Menu.as_str(),
        BookmarkRootGuid::Mobile.as_str(),
        BookmarkRootGuid::Toolbar.as_str(),
        BookmarkRootGuid::Unfiled.as_str(),
    ))?;
    reset_in_tx(db, &EngineSyncAssociation::Disconnected)?;
    tx.commit()?;
    Ok(())
}

/// A "raw" bookmark - a representation of the row and some summary fields.
#[derive(Debug)]
pub(crate) struct RawBookmark {
    pub place_id: Option<RowId>,
    pub row_id: RowId,
    pub bookmark_type: BookmarkType,
    pub parent_id: Option<RowId>,
    pub parent_guid: Option<SyncGuid>,
    pub position: u32,
    pub title: Option<String>,
    pub url: Option<Url>,
    pub date_added: Timestamp,
    pub date_modified: Timestamp,
    pub guid: SyncGuid,
    pub _sync_status: SyncStatus,
    pub _sync_change_counter: u32,
    pub child_count: u32,
    pub _grandparent_id: Option<RowId>,
}

impl RawBookmark {
    pub fn from_row(row: &Row<'_>) -> Result<Self> {
        let place_id = row.get::<_, Option<RowId>>("fk")?;
        Ok(Self {
            row_id: row.get("_id")?,
            place_id,
            bookmark_type: BookmarkType::from_u8_with_valid_url(row.get::<_, u8>("type")?, || {
                place_id.is_some()
            }),
            parent_id: row.get("_parentId")?,
            parent_guid: row.get("parentGuid")?,
            position: row.get("position")?,
            title: row.get::<_, Option<String>>("title")?,
            url: match row.get::<_, Option<String>>("url")? {
                Some(s) => Some(Url::parse(&s)?),
                None => None,
            },
            date_added: row.get("dateAdded")?,
            date_modified: row.get("lastModified")?,
            guid: row.get::<_, String>("guid")?.into(),
            _sync_status: SyncStatus::from_u8(row.get::<_, u8>("_syncStatus")?),
            _sync_change_counter: row
                .get::<_, Option<u32>>("syncChangeCounter")?
                .unwrap_or_default(),
            child_count: row.get("_childCount")?,
            _grandparent_id: row.get("_grandparentId")?,
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

pub(crate) fn get_raw_bookmark(db: &PlacesDb, guid: &SyncGuid) -> Result<Option<RawBookmark>> {
    // sql is based on fetchBookmark() in Desktop's Bookmarks.jsm, with 'fk' added
    // and title's NULLIF handling.
    db.try_query_row(
        &format!("{} WHERE b.guid = :guid", RAW_BOOKMARK_SQL),
        &[(":guid", guid)],
        RawBookmark::from_row,
        true,
    )
}

fn get_raw_bookmarks_for_url(db: &PlacesDb, url: &Url) -> Result<Vec<RawBookmark>> {
    db.query_rows_into_cached(
        &format!(
            "{} WHERE h.url_hash = hash(:url) AND h.url = :url",
            RAW_BOOKMARK_SQL
        ),
        &[(":url", &url.as_str())],
        RawBookmark::from_row,
    )
}

fn reset_in_tx(db: &PlacesDb, assoc: &EngineSyncAssociation) -> Result<()> {
    // Remove all synced bookmarks and pending tombstones, and mark all
    // local bookmarks as new.
    db.execute_batch(&format!(
        "DELETE FROM moz_bookmarks_synced;

        DELETE FROM moz_bookmarks_deleted;

        UPDATE moz_bookmarks
        SET syncChangeCounter = 1,
            syncStatus = {}",
        (SyncStatus::New as u8)
    ))?;

    // Recreate the set of synced roots, since we just removed all synced
    // bookmarks.
    bookmark_sync::create_synced_bookmark_roots(db)?;

    // Reset the last sync time, so that the next sync fetches fresh records
    // from the server.
    put_meta(db, LAST_SYNC_META_KEY, &0)?;

    // Clear the sync ID if we're signing out, or set it to whatever the
    // server gave us if we're signing in.
    match assoc {
        EngineSyncAssociation::Disconnected => {
            delete_meta(db, GLOBAL_SYNCID_META_KEY)?;
            delete_meta(db, COLLECTION_SYNCID_META_KEY)?;
        }
        EngineSyncAssociation::Connected(ids) => {
            put_meta(db, GLOBAL_SYNCID_META_KEY, &ids.global)?;
            put_meta(db, COLLECTION_SYNCID_META_KEY, &ids.coll)?;
        }
    }

    Ok(())
}

pub mod bookmark_sync {
    use super::*;
    use crate::bookmark_sync::SyncedBookmarkKind;

    /// Removes all sync metadata, including synced bookmarks, pending tombstones,
    /// change counters, sync statuses, the last sync time, and sync ID. This
    /// should be called when the user signs out of Sync.
    pub(crate) fn reset(db: &PlacesDb, assoc: &EngineSyncAssociation) -> Result<()> {
        let tx = db.begin_transaction()?;
        reset_in_tx(db, assoc)?;
        tx.commit()?;
        Ok(())
    }

    /// Sets up the syncable roots. All items in `moz_bookmarks_synced` descend
    /// from these roots.
    pub fn create_synced_bookmark_roots(db: &Connection) -> rusqlite::Result<()> {
        // NOTE: This is called in a transaction.
        fn maybe_insert(
            db: &Connection,
            guid: &SyncGuid,
            parent_guid: &SyncGuid,
            pos: u32,
        ) -> rusqlite::Result<()> {
            db.execute_batch(&format!(
                "INSERT OR IGNORE INTO moz_bookmarks_synced(guid, parentGuid, kind)
                VALUES('{guid}', '{parent_guid}', {kind});

                INSERT OR IGNORE INTO moz_bookmarks_synced_structure(
                    guid, parentGuid, position)
                VALUES('{guid}', '{parent_guid}', {pos});",
                guid = guid.as_str(),
                parent_guid = parent_guid.as_str(),
                kind = SyncedBookmarkKind::Folder as u8,
                pos = pos
            ))?;
            Ok(())
        }

        // The Places root is its own parent, to ensure it's always in
        // `moz_bookmarks_synced_structure`.
        maybe_insert(
            db,
            &BookmarkRootGuid::Root.as_guid(),
            &BookmarkRootGuid::Root.as_guid(),
            0,
        )?;
        for (pos, user_root) in USER_CONTENT_ROOTS.iter().enumerate() {
            maybe_insert(
                db,
                &user_root.as_guid(),
                &BookmarkRootGuid::Root.as_guid(),
                pos as u32,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::test::new_mem_connection;
    use crate::db::PlacesDb;
    use crate::storage::get_meta;
    use crate::tests::{append_invalid_bookmark, assert_json_tree, insert_json_tree};
    use json_tree::*;
    use pretty_assertions::assert_eq;
    use serde_json::Value;
    use std::collections::HashSet;

    fn get_pos(conn: &PlacesDb, guid: &SyncGuid) -> u32 {
        get_raw_bookmark(conn, guid)
            .expect("should work")
            .unwrap()
            .position
    }

    #[test]
    fn test_bookmark_url_for_keyword() -> Result<()> {
        let conn = new_mem_connection();

        let url = Url::parse("http://example.com").expect("valid url");

        conn.execute_cached(
            "INSERT INTO moz_places (guid, url, url_hash) VALUES ('fake_guid___', :url, hash(:url))",
            &[(":url", &String::from(url))],
        )
        .expect("should work");
        let place_id = conn.last_insert_rowid();

        // create a bookmark with keyword 'donut' pointing at it.
        conn.execute_cached(
            "INSERT INTO moz_keywords
                (keyword, place_id)
            VALUES
                ('donut', :place_id)",
            &[(":place_id", &place_id)],
        )
        .expect("should work");

        assert_eq!(
            bookmarks_get_url_for_keyword(&conn, "donut")?,
            Some(Url::parse("http://example.com")?)
        );
        assert_eq!(bookmarks_get_url_for_keyword(&conn, "juice")?, None);

        // now change the keyword to 'ice cream'
        conn.execute_cached(
            "REPLACE INTO moz_keywords
                (keyword, place_id)
            VALUES
                ('ice cream', :place_id)",
            &[(":place_id", &place_id)],
        )
        .expect("should work");

        assert_eq!(
            bookmarks_get_url_for_keyword(&conn, "ice cream")?,
            Some(Url::parse("http://example.com")?)
        );
        assert_eq!(bookmarks_get_url_for_keyword(&conn, "donut")?, None);
        assert_eq!(bookmarks_get_url_for_keyword(&conn, "ice")?, None);

        Ok(())
    }

    #[test]
    fn test_bookmark_invalid_url_for_keyword() -> Result<()> {
        let conn = new_mem_connection();

        let place_id =
            append_invalid_bookmark(&conn, BookmarkRootGuid::Unfiled.guid(), "invalid", "badurl")
                .place_id;

        // create a bookmark with keyword 'donut' pointing at it.
        conn.execute_cached(
            "INSERT INTO moz_keywords
                (keyword, place_id)
            VALUES
                ('donut', :place_id)",
            &[(":place_id", &place_id)],
        )
        .expect("should work");

        assert_eq!(bookmarks_get_url_for_keyword(&conn, "donut")?, None);

        Ok(())
    }

    #[test]
    fn test_insert() -> Result<()> {
        let conn = new_mem_connection();
        let url = Url::parse("https://www.example.com")?;

        conn.execute("UPDATE moz_bookmarks SET syncChangeCounter = 0", [])
            .expect("should work");

        let global_change_tracker = conn.global_bookmark_change_tracker();
        assert!(!global_change_tracker.changed(), "can't start as changed!");
        let bm = InsertableItem::Bookmark {
            b: InsertableBookmark {
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: BookmarkPosition::Append,
                date_added: None,
                last_modified: None,
                guid: None,
                url: url.clone(),
                title: Some("the title".into()),
            },
        };
        let guid = insert_bookmark(&conn, bm)?;

        // re-fetch it.
        let rb = get_raw_bookmark(&conn, &guid)?.expect("should get the bookmark");

        assert!(rb.place_id.is_some());
        assert_eq!(rb.bookmark_type, BookmarkType::Bookmark);
        assert_eq!(rb.parent_guid.unwrap(), BookmarkRootGuid::Unfiled);
        assert_eq!(rb.position, 0);
        assert_eq!(rb.title, Some("the title".into()));
        assert_eq!(rb.url, Some(url));
        assert_eq!(rb._sync_status, SyncStatus::New);
        assert_eq!(rb._sync_change_counter, 1);
        assert!(global_change_tracker.changed());
        assert_eq!(rb.child_count, 0);

        let unfiled = get_raw_bookmark(&conn, &BookmarkRootGuid::Unfiled.as_guid())?
            .expect("should get unfiled");
        assert_eq!(unfiled._sync_change_counter, 1);

        Ok(())
    }

    #[test]
    fn test_insert_titles() -> Result<()> {
        let conn = new_mem_connection();
        let url = Url::parse("https://www.example.com")?;

        let bm = InsertableItem::Bookmark {
            b: InsertableBookmark {
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: BookmarkPosition::Append,
                date_added: None,
                last_modified: None,
                guid: None,
                url: url.clone(),
                title: Some("".into()),
            },
        };
        let guid = insert_bookmark(&conn, bm)?;
        let rb = get_raw_bookmark(&conn, &guid)?.expect("should get the bookmark");
        assert_eq!(rb.title, None);

        let bm2 = InsertableItem::Bookmark {
            b: InsertableBookmark {
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: BookmarkPosition::Append,
                date_added: None,
                last_modified: None,
                guid: None,
                url,
                title: None,
            },
        };
        let guid2 = insert_bookmark(&conn, bm2)?;
        let rb2 = get_raw_bookmark(&conn, &guid2)?.expect("should get the bookmark");
        assert_eq!(rb2.title, None);
        Ok(())
    }

    #[test]
    fn test_delete() -> Result<()> {
        let conn = new_mem_connection();

        let guid1 = SyncGuid::random();
        let guid2 = SyncGuid::random();
        let guid2_1 = SyncGuid::random();
        let guid3 = SyncGuid::random();

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

        conn.execute(
            &format!(
                "UPDATE moz_bookmarks SET syncChangeCounter = 1, syncStatus = {}",
                SyncStatus::Normal as u8
            ),
            [],
        )
        .expect("should work");

        // Make sure the positions are correct now.
        assert_eq!(get_pos(&conn, &guid1), 0);
        assert_eq!(get_pos(&conn, &guid2), 1);
        assert_eq!(get_pos(&conn, &guid3), 2);

        let global_change_tracker = conn.global_bookmark_change_tracker();

        // Delete the middle folder.
        delete_bookmark(&conn, &guid2)?;
        // Should no longer exist.
        assert!(get_raw_bookmark(&conn, &guid2)?.is_none());
        // Neither should the child.
        assert!(get_raw_bookmark(&conn, &guid2_1)?.is_none());
        // Positions of the remaining should be correct.
        assert_eq!(get_pos(&conn, &guid1), 0);
        assert_eq!(get_pos(&conn, &guid3), 1);
        assert!(global_change_tracker.changed());

        assert_eq!(
            conn.query_one::<i64>(
                "SELECT COUNT(*) FROM moz_origins WHERE host='www.example2.com';"
            )?,
            0
        );

        Ok(())
    }

    #[test]
    fn test_delete_roots() {
        let conn = new_mem_connection();

        delete_bookmark(&conn, &BookmarkRootGuid::Root.into()).expect_err("can't delete root");
        delete_bookmark(&conn, &BookmarkRootGuid::Unfiled.into())
            .expect_err("can't delete any root");
    }

    #[test]
    fn test_insert_pos_too_large() -> Result<()> {
        let conn = new_mem_connection();
        let url = Url::parse("https://www.example.com")?;

        let bm = InsertableItem::Bookmark {
            b: InsertableBookmark {
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: BookmarkPosition::Specific { pos: 100 },
                date_added: None,
                last_modified: None,
                guid: None,
                url,
                title: Some("the title".into()),
            },
        };
        let guid = insert_bookmark(&conn, bm)?;

        // re-fetch it.
        let rb = get_raw_bookmark(&conn, &guid)?.expect("should get the bookmark");

        assert_eq!(rb.position, 0, "large value should have been ignored");
        Ok(())
    }

    #[test]
    fn test_update_move_same_parent() {
        let conn = new_mem_connection();
        let unfiled = &BookmarkRootGuid::Unfiled.as_guid();

        // A helper to make the moves below more concise.
        let do_move = |guid: &str, pos: BookmarkPosition| {
            let global_change_tracker = conn.global_bookmark_change_tracker();
            update_bookmark(
                &conn,
                &guid.into(),
                &UpdatableBookmark {
                    location: UpdateTreeLocation::Position { pos },
                    ..Default::default()
                }
                .into(),
            )
            .expect("update should work");
            assert!(global_change_tracker.changed(), "should be tracked");
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
        do_move("bookmark3___", BookmarkPosition::Specific { pos: 1 });
        check_tree(json!([
            {"url": "https://www.example1.com/"},
            {"url": "https://www.example3.com/"},
            {"url": "https://www.example2.com/"},
        ]));

        // Move a bookmark back 1 position.
        do_move("bookmark2___", BookmarkPosition::Specific { pos: 1 });
        check_tree(json!([
            {"url": "https://www.example1.com/"},
            {"url": "https://www.example2.com/"},
            {"url": "https://www.example3.com/"},
        ]));

        // Move a bookmark forward 1 position.
        do_move("bookmark2___", BookmarkPosition::Specific { pos: 2 });
        check_tree(json!([
            {"url": "https://www.example1.com/"},
            {"url": "https://www.example3.com/"},
            {"url": "https://www.example2.com/"},
        ]));

        // Move a bookmark beyond the end.
        do_move("bookmark1___", BookmarkPosition::Specific { pos: 10 });
        check_tree(json!([
            {"url": "https://www.example3.com/"},
            {"url": "https://www.example2.com/"},
            {"url": "https://www.example1.com/"},
        ]));
    }

    #[test]
    fn test_update() -> Result<()> {
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
                location: UpdateTreeLocation::Position {
                    pos: BookmarkPosition::Specific { pos: 2 },
                },
                ..Default::default()
            }
            .into(),
        )?;

        // A move across folders.
        update_bookmark(
            &conn,
            &"bookmark2___".into(),
            &UpdatableBookmark {
                location: UpdateTreeLocation::Parent {
                    guid: "folder1_____".into(),
                    pos: BookmarkPosition::Specific { pos: 1 },
                },
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

        conn.execute("UPDATE moz_bookmarks SET syncChangeCounter = 0", [])
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
        assert_eq!(bm._sync_change_counter, 0);

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
        assert_eq!(bm._sync_change_counter, 0);

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
        assert_eq!(bm._sync_change_counter, 1);

        Ok(())
    }

    #[test]
    fn test_update_statuses() -> Result<()> {
        let conn = new_mem_connection();
        let unfiled = &BookmarkRootGuid::Unfiled.as_guid();

        let check_change_counters = |guids: Vec<&str>| {
            let sql = "SELECT guid FROM moz_bookmarks WHERE syncChangeCounter != 0";
            let mut stmt = conn.prepare(sql).expect("sql is ok");
            let got_guids: HashSet<String> = stmt
                .query_and_then([], |row| -> rusqlite::Result<_> { row.get::<_, String>(0) })
                .expect("should work")
                .map(std::result::Result::unwrap)
                .collect();

            assert_eq!(
                got_guids,
                guids.into_iter().map(ToString::to_string).collect()
            );
            // reset them all back
            conn.execute("UPDATE moz_bookmarks SET syncChangeCounter = 0", [])
                .expect("should work");
        };

        let check_last_modified = |guids: Vec<&str>| {
            let sql = "SELECT guid FROM moz_bookmarks
                       WHERE lastModified >= 1000 AND guid != 'root________'";

            let mut stmt = conn.prepare(sql).expect("sql is ok");
            let got_guids: HashSet<String> = stmt
                .query_and_then([], |row| -> rusqlite::Result<_> { row.get::<_, String>(0) })
                .expect("should work")
                .map(std::result::Result::unwrap)
                .collect();

            assert_eq!(
                got_guids,
                guids.into_iter().map(ToString::to_string).collect()
            );
            // reset them all back
            conn.execute("UPDATE moz_bookmarks SET lastModified = 123", [])
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
            [],
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
                location: UpdateTreeLocation::Position {
                    pos: BookmarkPosition::Append,
                },
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
                location: UpdateTreeLocation::Parent {
                    guid: "folder2_____".into(),
                    pos: BookmarkPosition::Append,
                },
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
    fn test_update_errors() {
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
                location: UpdateTreeLocation::Parent {
                    guid: "bookmark2___".into(),
                    pos: BookmarkPosition::Specific { pos: 1 },
                },
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
                location: UpdateTreeLocation::Parent {
                    guid: BookmarkRootGuid::Root.as_guid(),
                    pos: BookmarkPosition::Specific { pos: 1 },
                },
                ..Default::default()
            }
            .into(),
        )
        .expect_err("can't move to the root");
    }

    #[test]
    fn test_delete_everything() -> Result<()> {
        let conn = new_mem_connection();

        insert_bookmark(
            &conn,
            InsertableFolder {
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: BookmarkPosition::Append,
                date_added: None,
                last_modified: None,
                guid: Some("folderAAAAAA".into()),
                title: Some("A".into()),
                children: vec![],
            }
            .into(),
        )?;
        insert_bookmark(
            &conn,
            InsertableBookmark {
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: BookmarkPosition::Append,
                date_added: None,
                last_modified: None,
                guid: Some("bookmarkBBBB".into()),
                url: Url::parse("http://example.com/b")?,
                title: Some("B".into()),
            }
            .into(),
        )?;
        insert_bookmark(
            &conn,
            InsertableBookmark {
                parent_guid: "folderAAAAAA".into(),
                position: BookmarkPosition::Append,
                date_added: None,
                last_modified: None,
                guid: Some("bookmarkCCCC".into()),
                url: Url::parse("http://example.com/c")?,
                title: Some("C".into()),
            }
            .into(),
        )?;

        delete_everything(&conn)?;

        let (tree, _, _) =
            fetch_tree(&conn, &BookmarkRootGuid::Root.into(), &FetchDepth::Deepest)?.unwrap();
        if let BookmarkTreeNode::Folder { f: root } = tree {
            assert_eq!(root.children.len(), 4);
            let unfiled = root
                .children
                .iter()
                .find(|c| c.guid() == BookmarkRootGuid::Unfiled.guid())
                .expect("Should return unfiled root");
            if let BookmarkTreeNode::Folder { f: unfiled } = unfiled {
                assert!(unfiled.children.is_empty());
            } else {
                panic!("The unfiled root should be a folder");
            }
        } else {
            panic!("`fetch_tree` should return the Places root folder");
        }

        Ok(())
    }

    #[test]
    fn test_sync_reset() -> Result<()> {
        let conn = new_mem_connection();

        // Add Sync metadata keys, to ensure they're reset.
        put_meta(&conn, GLOBAL_SYNCID_META_KEY, &"syncAAAAAAAA")?;
        put_meta(&conn, COLLECTION_SYNCID_META_KEY, &"syncBBBBBBBB")?;
        put_meta(&conn, LAST_SYNC_META_KEY, &12345)?;

        insert_bookmark(
            &conn,
            InsertableBookmark {
                parent_guid: BookmarkRootGuid::Unfiled.into(),
                position: BookmarkPosition::Append,
                date_added: None,
                last_modified: None,
                guid: Some("bookmarkAAAA".into()),
                url: Url::parse("http://example.com/a")?,
                title: Some("A".into()),
            }
            .into(),
        )?;

        // Mark all items as synced.
        conn.execute(
            &format!(
                "UPDATE moz_bookmarks SET
                     syncChangeCounter = 0,
                     syncStatus = {}",
                (SyncStatus::Normal as u8)
            ),
            [],
        )?;

        let bmk = get_raw_bookmark(&conn, &"bookmarkAAAA".into())?
            .expect("Should fetch A before resetting");
        assert_eq!(bmk._sync_change_counter, 0);
        assert_eq!(bmk._sync_status, SyncStatus::Normal);

        bookmark_sync::reset(&conn, &EngineSyncAssociation::Disconnected)?;

        let bmk = get_raw_bookmark(&conn, &"bookmarkAAAA".into())?
            .expect("Should fetch A after resetting");
        assert_eq!(bmk._sync_change_counter, 1);
        assert_eq!(bmk._sync_status, SyncStatus::New);

        // Ensure we reset Sync metadata, too.
        let global = get_meta::<SyncGuid>(&conn, GLOBAL_SYNCID_META_KEY)?;
        assert!(global.is_none());
        let coll = get_meta::<SyncGuid>(&conn, COLLECTION_SYNCID_META_KEY)?;
        assert!(coll.is_none());
        let since = get_meta::<i64>(&conn, LAST_SYNC_META_KEY)?;
        assert_eq!(since, Some(0));

        Ok(())
    }

    #[test]
    fn test_count_tree() -> Result<()> {
        let conn = new_mem_connection();
        let unfiled = BookmarkRootGuid::Unfiled.as_guid();

        insert_json_tree(
            &conn,
            json!({
                "guid": &unfiled,
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
                                "guid": "separator1__",
                                "type": BookmarkType::Separator,
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
                    {
                        "guid": "folder3_____",
                        "title": "Another folder",
                        "children": [
                            {
                                "guid": "bookmark3___",
                                "title": "bookmark in folder 3",
                                "url": "https://www.example2.com/"
                            },
                            {
                                "guid": "separator2__",
                                "type": BookmarkType::Separator,
                            },
                            {
                                "guid": "bookmark4___",
                                "title": "next bookmark in folder 3",
                                "url": "https://www.example3.com/"
                            },
                        ]
                    },
                ]
            }),
        );
        assert_eq!(count_bookmarks_in_trees(&conn, &[])?, 0);
        // A folder with sub-folders
        assert_eq!(count_bookmarks_in_trees(&conn, &[unfiled])?, 4);
        // A folder with items but no folders.
        assert_eq!(
            count_bookmarks_in_trees(&conn, &[SyncGuid::from("folder1_____")])?,
            2
        );
        // Asking for a bookmark (ie, not a folder) or an invalid guid gives zero.
        assert_eq!(
            count_bookmarks_in_trees(&conn, &[SyncGuid::from("bookmark1___")])?,
            0
        );
        assert_eq!(
            count_bookmarks_in_trees(&conn, &[SyncGuid::from("no_such_guid")])?,
            0
        );
        // empty folder also zero.
        assert_eq!(
            count_bookmarks_in_trees(&conn, &[SyncGuid::from("folder2_____")])?,
            0
        );
        // multiple folders
        assert_eq!(
            count_bookmarks_in_trees(
                &conn,
                &[
                    SyncGuid::from("folder1_____"),
                    SyncGuid::from("folder3_____")
                ]
            )?,
            4
        );
        Ok(())
    }
}

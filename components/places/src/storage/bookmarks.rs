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
use std::iter::Peekable;
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
    // desktop's sql seems to assume the root gets a rowid of zero, whereas
    // we see 1 here. Regardless, the sql below uses the guid.
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

#[derive(Debug, Clone)]
pub struct InsertableSeparator {
    pub parent_guid: SyncGuid,
    pub position: BookmarkPosition,
    pub date_added: Option<Timestamp>,
    pub last_modified: Option<Timestamp>,
    pub guid: Option<SyncGuid>,
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

// The type used to insert the actual item.
#[derive(Debug, Clone)]
pub enum InsertableBookmarkItem {
    Bookmark(InsertableBookmark),
    Separator(InsertableSeparator),
    Folder(InsertableFolder),
}

// We allow all "common" fields from the sub-types to be getters on the
// InsertableBookmarkItem type.
macro_rules! impl_common_bookmark_getter {
    ($getter_name:ident, $T:ty) => {
        fn $getter_name(&self) -> &$T {
            match self {
                InsertableBookmarkItem::Bookmark(b) => &b.$getter_name,
                InsertableBookmarkItem::Separator(s) => &s.$getter_name,
                InsertableBookmarkItem::Folder(f) => &f.$getter_name,
            }
        }
    };
}

impl InsertableBookmarkItem {
    fn bookmark_type(&self) -> BookmarkType {
        match self {
            InsertableBookmarkItem::Bookmark(_) => BookmarkType::Bookmark,
            InsertableBookmarkItem::Separator(_) => BookmarkType::Separator,
            InsertableBookmarkItem::Folder(_) => BookmarkType::Folder,
        }
    }
    impl_common_bookmark_getter!(parent_guid, SyncGuid);
    impl_common_bookmark_getter!(position, BookmarkPosition);
    impl_common_bookmark_getter!(date_added, Option<Timestamp>);
    impl_common_bookmark_getter!(last_modified, Option<Timestamp>);
    impl_common_bookmark_getter!(guid, Option<SyncGuid>);
}

pub fn insert_bookmark(db: &impl ConnExt, bm: InsertableBookmarkItem) -> Result<SyncGuid> {
    let tx = db.unchecked_transaction()?;
    let result = do_insert_bookmark(db, bm);
    match result {
        Ok(_) => tx.commit()?,
        Err(_) => tx.rollback()?,
    }
    result
}

fn do_insert_bookmark(db: &impl ConnExt, bm: InsertableBookmarkItem) -> Result<SyncGuid> {
    // find the row ID of the parent.
    if BookmarkRootGuid::from_guid(&bm.parent_guid()) == Some(BookmarkRootGuid::Root) {
        return Err(InvalidPlaceInfo::InvalidGuid.into());
    }
    let parent = match get_raw_bookmark(db, &bm.parent_guid())? {
        Some(p) => p,
        None => {
            log::warn!(
                "Can't insert item with parent '{:?}' as the parent doesn't exist",
                bm.parent_guid()
            );
            return Err(InvalidPlaceInfo::InvalidParent.into());
        }
    };
    if parent.bookmark_type != BookmarkType::Folder {
        return Err(InvalidPlaceInfo::InvalidParent.into());
    }
    // Do the "position" dance.
    let position: u32 = match *bm.position() {
        BookmarkPosition::Specific(specified) => {
            let actual = min(specified, parent.child_count);
            // must reorder existing children.
            db.execute_named_cached(
                "UPDATE moz_bookmarks SET position = position + 1
                 WHERE parent = :parent
                 AND position >= :position",
                &[(":parent", &parent.parent_id), (":position", &actual)],
            )?;
            actual
        }
        BookmarkPosition::Append => parent.child_count,
    };
    // Note that we could probably do this 'fk' work as a sub-query (although
    // markh isn't clear how we could perform the insert) - it probably doesn't
    // matter in practice though...
    let fk = match bm {
        InsertableBookmarkItem::Bookmark(ref bm) => {
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
    let params: Vec<(&str, &ToSql)> = match bm {
        InsertableBookmarkItem::Bookmark(ref b) => vec![
            (":fk", &fk),
            (":type", &bookmark_type),
            (":parent", &parent.row_id),
            (":position", &position),
            (":title", &b.title),
            (":dateAdded", &date_added),
            (":lastModified", &last_modified),
            (":guid", &guid),
            (":syncStatus", &SyncStatus::New),
            (":syncChangeCounter", &1),
        ],
        InsertableBookmarkItem::Separator(ref _s) => vec![
            (":type", &bookmark_type),
            (":parent", &parent.row_id),
            (":position", &position),
            (":dateAdded", &date_added),
            (":lastModified", &last_modified),
            (":guid", &guid),
            (":syncStatus", &SyncStatus::New),
            (":syncChangeCounter", &1),
        ],
        InsertableBookmarkItem::Folder(ref f) => vec![
            (":type", &bookmark_type),
            (":parent", &parent.row_id),
            (":title", &f.title),
            (":position", &position),
            (":dateAdded", &date_added),
            (":lastModified", &last_modified),
            (":guid", &guid),
            (":syncStatus", &SyncStatus::New),
            (":syncChangeCounter", &1),
        ],
    };
    db.execute_named_cached(sql, &params)?;
    Ok(guid)
}

/// Support for inserting and fetching a tree. Same limitations as desktop.
/// Note that the guids are optional when inserting a tree. They will always
/// have values when fetching it.

// For testing purposes we implement PartialEq, such that optional fields are
// ignored in the comparison. This allows tests to construct a tree with
// missing fields and still be able to compare an exported tree (with all
// fields) against the initial one.
macro_rules! cmp_option {
    ($s: ident, $o: ident, $name:ident) => {
        match (&$s.$name, &$o.$name) {
            (None, None) => true,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (s, o) => s == o,
        }
    };
}

#[derive(Debug)]
pub struct BookmarkNode {
    pub guid: Option<SyncGuid>,
    pub date_added: Option<Timestamp>,
    pub last_modified: Option<Timestamp>,
    pub title: Option<String>,
    pub url: Url,
}

// #[test] - XXX - we only need these for tests and it would be preferable
// to not expose otherise - but this gets `error: only functions may be used as tests`
impl PartialEq for BookmarkNode {
    fn eq(&self, other: &BookmarkNode) -> bool {
        cmp_option!(self, other, guid)
            && cmp_option!(self, other, date_added)
            && cmp_option!(self, other, last_modified)
            && cmp_option!(self, other, title)
            && self.url == other.url
    }
}

#[derive(Debug, Default)]
pub struct SeparatorNode {
    pub guid: Option<SyncGuid>,
    pub date_added: Option<Timestamp>,
    pub last_modified: Option<Timestamp>,
}

impl PartialEq for SeparatorNode {
    fn eq(&self, other: &SeparatorNode) -> bool {
        cmp_option!(self, other, guid)
            && cmp_option!(self, other, date_added)
            && cmp_option!(self, other, last_modified)
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

impl PartialEq for FolderNode {
    fn eq(&self, other: &FolderNode) -> bool {
        cmp_option!(self, other, guid)
            && cmp_option!(self, other, date_added)
            && cmp_option!(self, other, last_modified)
            && cmp_option!(self, other, title)
            && self.children == other.children
    }
}

#[derive(Debug, PartialEq)]
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
            #[serde(with = "url_serde")]
            url: Option<Url>,
            children: Vec<BookmarkTreeNode>,
        }
        let m = Mapping::deserialize(deserializer)?;

        // this patten has been copy-pasta'd too often...
        let bookmark_type = match BookmarkType::from_u8(m.bookmark_type) {
            Some(t) => t,
            None => match m.url {
                Some(_) => BookmarkType::Bookmark,
                _ => BookmarkType::Folder,
            },
        };

        Ok(match bookmark_type {
            BookmarkType::Bookmark => {
                BookmarkTreeNode::Bookmark(BookmarkNode {
                    guid: m.guid,
                    date_added: m.date_added,
                    last_modified: m.last_modified,
                    title: m.title,
                    // XXX - need to handle None and invalid URLs
                    url: m.url.unwrap(),
                })
            }
            BookmarkType::Separator => BookmarkTreeNode::Separator(SeparatorNode {
                guid: m.guid,
                date_added: m.date_added,
                last_modified: m.last_modified,
            }),
            BookmarkType::Folder => BookmarkTreeNode::Folder(FolderNode {
                guid: m.guid,
                date_added: m.date_added,
                last_modified: m.last_modified,
                title: m.title,
                children: m.children,
            }),
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
}

fn add_subtree_infos(
    db: &impl ConnExt,
    parent: &SyncGuid,
    tree: &FolderNode,
    insert_infos: &mut Vec<InsertableBookmarkItem>,
) -> Result<()> {
    // TODO: track last modified? Like desktop, we should probably have
    // the default values passed in so the entire tree has consistent
    // timestamps.
    let default_when = Some(Timestamp::now());
    insert_infos.reserve(tree.children.len());
    for child in &tree.children {
        match child {
            BookmarkTreeNode::Bookmark(b) => {
                insert_infos.push(InsertableBookmarkItem::Bookmark(InsertableBookmark {
                    parent_guid: parent.clone(),
                    position: BookmarkPosition::Append,
                    date_added: b.date_added.or(default_when),
                    last_modified: b.last_modified.or(default_when),
                    guid: b.guid.clone(),
                    url: b.url.clone(),
                    title: b.title.clone(),
                }))
            }
            BookmarkTreeNode::Separator(s) => {
                insert_infos.push(InsertableBookmarkItem::Separator(InsertableSeparator {
                    parent_guid: parent.clone(),
                    position: BookmarkPosition::Append,
                    date_added: s.date_added.or(default_when),
                    last_modified: s.last_modified.or(default_when),
                    guid: s.guid.clone(),
                }))
            }
            BookmarkTreeNode::Folder(f) => {
                let my_guid = f.guid.clone().unwrap_or_else(|| SyncGuid::new());
                // must add the folder before we recurse into children.
                insert_infos.push(InsertableBookmarkItem::Folder(InsertableFolder {
                    parent_guid: parent.clone(),
                    position: BookmarkPosition::Append,
                    date_added: f.date_added.or(default_when),
                    last_modified: f.last_modified.or(default_when),
                    guid: Some(my_guid.clone()),
                    title: f.title.clone(),
                }));
                add_subtree_infos(db, &my_guid, &f, insert_infos)?;
            }
        };
    }
    Ok(())
}

pub fn insert_tree(db: &impl ConnExt, tree: &FolderNode) -> Result<()> {
    let parent_guid = match &tree.guid {
        Some(guid) => guid,
        None => return Err(InvalidPlaceInfo::InvalidParent.into()),
    };

    let mut insert_infos: Vec<InsertableBookmarkItem> = Vec::new();
    add_subtree_infos(db, &parent_guid, tree, &mut insert_infos)?;
    log::info!("insert_tree inserting {} records", insert_infos.len());
    let tx = db.unchecked_transaction()?;

    for insertable in insert_infos {
        do_insert_bookmark(db, insertable)?;
    }
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
            parent_guid: match row.get_checked::<_, Option<String>>("parentGuid")? {
                Some(g) => Some(SyncGuid(g)),
                None => None,
            },
            node_type: match BookmarkType::from_u8(row.get_checked::<_, u8>("type")?) {
                Some(t) => t,
                None => match url {
                    Some(_) => BookmarkType::Bookmark,
                    _ => BookmarkType::Folder,
                },
            },
            position: row.get_checked("position")?,
            title: row.get_checked::<_, Option<String>>("title")?,
            date_added: row.get_checked("dateAdded")?,
            last_modified: row.get_checked("lastModified")?,
            url,
        })
    }
}

/// A recursive function that processes a number of rows in an iterator.
/// The first row from the iterator must be that of a folder. Subsequent
/// rows are consumed while they have a level greater than the level of
/// the folder itself.
fn process_folder_rows<'a, I>(rows: &mut Peekable<I>) -> FolderNode
where
    I: Iterator<Item = FetchedTreeRow>,
{
    // Our query guarantees that we always visit parents ahead of their
    // children. This function is only called for folders.
    let folder_row = rows
        .next()
        .expect("should never be called with exhausted iter");
    let mut result = FolderNode {
        guid: Some(folder_row.guid.clone()),
        date_added: Some(folder_row.date_added),
        last_modified: Some(folder_row.last_modified),
        title: folder_row.title.clone(),
        children: Vec::new(),
    };
    let folder_level = folder_row.level;
    loop {
        let (next_level, next_type) = match rows.peek() {
            None => return result,
            Some(next_row) => (next_row.level, next_row.node_type),
        };

        if next_level <= folder_level {
            // next item is a sibling of our result folder, so we are done.
            return result;
        }

        let node = match next_type {
            BookmarkType::Folder => Some(BookmarkTreeNode::Folder(process_folder_rows(rows))),
            _ => {
                // not a folder, so we must consume the row.
                let row = match rows.next() {
                    Some(row) => row,
                    // None should be impossible as we already peeked at it!
                    None => return result, // iterator is exhaused.
                };
                match row.node_type {
                    BookmarkType::Bookmark => match &row.url {
                        Some(url_str) => match Url::parse(&url_str) {
                            Ok(url) => Some(BookmarkTreeNode::Bookmark(BookmarkNode {
                                guid: Some(row.guid.clone()),
                                date_added: Some(row.date_added),
                                last_modified: Some(row.last_modified),
                                title: row.title.clone(),
                                url,
                            })),
                            Err(e) => {
                                log::warn!(
                                    "ignoring malformed bookmark - invalid URL {}: {:?}",
                                    url_str,
                                    e
                                );
                                None
                            }
                        },
                        None => {
                            log::warn!("ignoring malformed bookmark {:?}- no URL", row);
                            None
                        }
                    },
                    BookmarkType::Separator => Some(BookmarkTreeNode::Separator(SeparatorNode {
                        guid: Some(row.guid.clone()),
                        date_added: Some(row.date_added),
                        last_modified: Some(row.last_modified),
                    })),
                    BookmarkType::Folder => panic!("impossible - we already peeked and checked"),
                }
            }
        };
        if let Some(node) = node {
            result.children.push(node);
        }
    }
    //unreachable!();
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

    let results =
        stmt.query_and_then_named(&[(":item_guid", item_guid)], FetchedTreeRow::from_row)?;
    // markh can't work out how to do this directly with an AndThenRows - so
    // we slurp the rows into a Vec and iterate over that.
    let mut rows = Vec::new();
    for result in results {
        rows.push(result?);
    }
    let mut peekable_row_iter = rows.into_iter().peekable();
    Ok(match peekable_row_iter.peek() {
        Some(_) => Some(BookmarkTreeNode::Folder(process_folder_rows(
            &mut peekable_row_iter,
        ))),
        None => None,
    })
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
            bookmark_type: match BookmarkType::from_u8(row.get_checked::<_, u8>("type")?) {
                Some(t) => t,
                None => {
                    if place_id.is_some() {
                        BookmarkType::Bookmark
                    } else {
                        BookmarkType::Folder
                    }
                }
            },
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

    #[test]
    fn test_insert() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = PlacesDb::open_in_memory(None)?;
        let url = Url::parse("https://www.example.com")?;

        let bm = InsertableBookmarkItem::Bookmark(InsertableBookmark {
            parent_guid: BookmarkRootGuid::Unfiled.as_guid(),
            position: BookmarkPosition::Append,
            date_added: None,
            last_modified: None,
            guid: None,
            url: url.clone(),
            title: Some("the title".into()),
        });
        let guid = insert_bookmark(&conn, bm)?;

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
    fn test_insert_pos_too_large() -> Result<()> {
        let _ = env_logger::try_init();
        let conn = PlacesDb::open_in_memory(None)?;
        let url = Url::parse("https://www.example.com")?;

        let bm = InsertableBookmarkItem::Bookmark(InsertableBookmark {
            parent_guid: BookmarkRootGuid::Unfiled.as_guid(),
            position: BookmarkPosition::Specific(100),
            date_added: None,
            last_modified: None,
            guid: None,
            url: url.clone(),
            title: Some("the title".into()),
        });
        let guid = insert_bookmark(&conn, bm)?;

        // re-fetch it.
        let rb = get_raw_bookmark(&conn, &guid)?.expect("should get the bookmark");

        assert_eq!(rb.position, 0, "large value should have been ignored");
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
        assert_eq!(f.guid, Some(BookmarkRootGuid::Root.as_guid()));
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
                BookmarkTreeNode::Bookmark(BookmarkNode {
                    guid: None,
                    date_added: None,
                    last_modified: None,
                    title: Some("the bookmark".into()),
                    url: Url::parse("https://www.example.com")?,
                }),
                BookmarkTreeNode::Folder(FolderNode {
                    title: Some("A folder".into()),
                    children: vec![BookmarkTreeNode::Bookmark(BookmarkNode {
                        guid: None,
                        date_added: None,
                        last_modified: None,
                        title: Some("bookmark in A folder".into()),
                        url: Url::parse("https://www.example2.com")?,
                    })],
                    ..Default::default()
                }),
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
                            "title": "bookmark in A folder",
                            "url": "https://www.example2.com/"
                        }
                    ]
                }
            ]
        });
        let deser_tree: BookmarkTreeNode = serde_json::from_value(expected)?;
        assert_eq!(fetched, deser_tree);
        Ok(())
    }
}

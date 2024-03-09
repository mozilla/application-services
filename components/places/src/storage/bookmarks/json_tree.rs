/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// This supports inserting and fetching an entire bookmark tree via JSON
// compatible data structures.
// It's currently used only by tests, examples and our utilities for importing
// from a desktop JSON exports.
//
// None of our "real" consumers currently require JSON compatibility, so try
// and avoid using this if you can!
// (We could possibly put this behind a feature flag?)

use crate::error::{warn, Result};
use crate::types::BookmarkType;
//#[cfg(test)]
use crate::db::PlacesDb;
use rusqlite::Row;
use sql_support::ConnExt;
use std::collections::HashMap;
use sync_guid::Guid as SyncGuid;
use types::Timestamp;
use url::Url;

use super::{
    BookmarkPosition, InsertableBookmark, InsertableFolder, InsertableItem, InsertableSeparator,
    RowId,
};

use serde::{
    de::{Deserialize, Deserializer},
    ser::{Serialize, SerializeStruct, Serializer},
};
use serde_derive::*;

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
    fn from(b: BookmarkNode) -> Self {
        BookmarkTreeNode::Bookmark { b }
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
    fn from(s: SeparatorNode) -> Self {
        BookmarkTreeNode::Separator { s }
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
    fn from(f: FolderNode) -> Self {
        BookmarkTreeNode::Folder { f }
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
    Bookmark { b: BookmarkNode },
    Separator { s: SeparatorNode },
    Folder { f: FolderNode },
}

impl BookmarkTreeNode {
    pub fn node_type(&self) -> BookmarkType {
        match self {
            BookmarkTreeNode::Bookmark { .. } => BookmarkType::Bookmark,
            BookmarkTreeNode::Folder { .. } => BookmarkType::Folder,
            BookmarkTreeNode::Separator { .. } => BookmarkType::Separator,
        }
    }

    pub fn guid(&self) -> &SyncGuid {
        let guid = match self {
            BookmarkTreeNode::Bookmark { b } => b.guid.as_ref(),
            BookmarkTreeNode::Folder { f } => f.guid.as_ref(),
            BookmarkTreeNode::Separator { s } => s.guid.as_ref(),
        };
        // Can this happen? Why is this an Option?
        guid.expect("Missing guid?")
    }

    pub fn created_modified(&self) -> (Timestamp, Timestamp) {
        let (created, modified) = match self {
            BookmarkTreeNode::Bookmark { b } => (b.date_added, b.last_modified),
            BookmarkTreeNode::Folder { f } => (f.date_added, f.last_modified),
            BookmarkTreeNode::Separator { s } => (s.date_added, s.last_modified),
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
            BookmarkTreeNode::Bookmark { b } => {
                state.serialize_field("type", &BookmarkType::Bookmark)?;
                state.serialize_field("guid", &b.guid)?;
                state.serialize_field("date_added", &b.date_added)?;
                state.serialize_field("last_modified", &b.last_modified)?;
                state.serialize_field("title", &b.title)?;
                state.serialize_field("url", &b.url.to_string())?;
            }
            BookmarkTreeNode::Separator { s } => {
                state.serialize_field("type", &BookmarkType::Separator)?;
                state.serialize_field("guid", &s.guid)?;
                state.serialize_field("date_added", &s.date_added)?;
                state.serialize_field("last_modified", &s.last_modified)?;
            }
            BookmarkTreeNode::Folder { f } => {
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
                warn!(
                    "ignoring invalid url for {}: {:?}",
                    m.guid.as_ref().map(AsRef::as_ref).unwrap_or("<no guid>"),
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

impl From<BookmarkTreeNode> for InsertableItem {
    fn from(node: BookmarkTreeNode) -> Self {
        match node {
            BookmarkTreeNode::Bookmark { b } => InsertableBookmark {
                parent_guid: SyncGuid::empty(),
                position: BookmarkPosition::Append,
                date_added: b.date_added,
                last_modified: b.last_modified,
                guid: b.guid,
                url: b.url,
                title: b.title,
            }
            .into(),
            BookmarkTreeNode::Separator { s } => InsertableSeparator {
                parent_guid: SyncGuid::empty(),
                position: BookmarkPosition::Append,
                date_added: s.date_added,
                last_modified: s.last_modified,
                guid: s.guid,
            }
            .into(),
            BookmarkTreeNode::Folder { f } => InsertableFolder {
                parent_guid: SyncGuid::empty(),
                position: BookmarkPosition::Append,
                date_added: f.date_added,
                last_modified: f.last_modified,
                guid: f.guid,
                title: f.title,
                children: f.children.into_iter().map(Into::into).collect(),
            }
            .into(),
        }
    }
}

#[cfg(test)]
mod test_serialize {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tree_serialize() -> Result<()> {
        let guid = SyncGuid::random();
        let tree = BookmarkTreeNode::Folder {
            f: FolderNode {
                guid: Some(guid.clone()),
                date_added: None,
                last_modified: None,
                title: None,
                children: vec![BookmarkTreeNode::Bookmark {
                    b: BookmarkNode {
                        guid: None,
                        date_added: None,
                        last_modified: None,
                        title: Some("the bookmark".into()),
                        url: Url::parse("https://www.example.com")?,
                    },
                }],
            },
        };
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
    fn test_tree_invalid() {
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
            BookmarkTreeNode::Folder { f } => f,
            _ => panic!("must be a folder"),
        };

        let children = folder.children;
        assert_eq!(children.len(), 4);

        assert!(match &children[0] {
            BookmarkTreeNode::Folder { f } =>
                f.title == Some("bookmark with invalid URL".to_string()),
            _ => false,
        });
        assert!(match &children[1] {
            BookmarkTreeNode::Folder { f } =>
                f.title == Some("bookmark with missing URL".to_string()),
            _ => false,
        });
        assert!(match &children[2] {
            BookmarkTreeNode::Folder { f } => {
                f.title == Some("bookmark with missing type, no URL".to_string())
            }
            _ => false,
        });
        assert!(match &children[3] {
            BookmarkTreeNode::Bookmark { b } => {
                b.title == Some("bookmark with missing type, valid URL".to_string())
            }
            _ => false,
        });
    }
}

pub fn insert_tree(db: &PlacesDb, tree: FolderNode) -> Result<()> {
    // This API is strange - we don't add `tree`, but just use it for the parent.
    // It's only used for json importing, so we can live with a strange API :)
    let parent = tree.guid.expect("inserting a tree without the root guid");
    let tx = db.begin_transaction()?;
    for child in tree.children {
        let mut insertable: InsertableItem = child.into();
        assert!(
            insertable.parent_guid().is_empty(),
            "can't specify a parent inserting a tree"
        );
        insertable.set_parent_guid(parent.clone());
        crate::storage::bookmarks::insert_bookmark_in_tx(db, insertable)?;
    }
    crate::storage::delete_pending_temp_tables(db)?;
    tx.commit()?;
    Ok(())
}

fn inflate(
    parent: &mut BookmarkTreeNode,
    pseudo_tree: &mut HashMap<SyncGuid, Vec<BookmarkTreeNode>>,
) {
    if let BookmarkTreeNode::Folder { f: parent } = parent {
        if let Some(children) = parent
            .guid
            .as_ref()
            .and_then(|guid| pseudo_tree.remove(guid))
        {
            parent.children = children;
            for child in &mut parent.children {
                inflate(child, pseudo_tree);
            }
        }
    }
}

#[derive(Debug)]
struct FetchedTreeRow {
    level: u32,
    _id: RowId,
    guid: SyncGuid,
    // parent and parent_guid are Option<> only to handle the root - we would
    // assert but they aren't currently used.
    _parent: Option<RowId>,
    parent_guid: Option<SyncGuid>,
    node_type: BookmarkType,
    position: u32,
    title: Option<String>,
    date_added: Timestamp,
    last_modified: Timestamp,
    url: Option<String>,
}

impl FetchedTreeRow {
    pub fn from_row(row: &Row<'_>) -> Result<Self> {
        let url = row.get::<_, Option<String>>("url")?;
        Ok(Self {
            level: row.get("level")?,
            _id: row.get::<_, RowId>("id")?,
            guid: row.get::<_, String>("guid")?.into(),
            _parent: row.get::<_, Option<RowId>>("parent")?,
            parent_guid: row
                .get::<_, Option<String>>("parentGuid")?
                .map(SyncGuid::from),
            node_type: BookmarkType::from_u8_with_valid_url(row.get::<_, u8>("type")?, || {
                url.is_some()
            }),
            position: row.get("position")?,
            title: row.get::<_, Option<String>>("title")?,
            date_added: row.get("dateAdded")?,
            last_modified: row.get("lastModified")?,
            url,
        })
    }
}

/// Fetch the tree starting at the specified guid.
/// Returns a `BookmarkTreeNode`, its parent's guid (if any), and
/// position inside its parent.
pub enum FetchDepth {
    Specific(usize),
    Deepest,
}

pub fn fetch_tree(
    db: &PlacesDb,
    item_guid: &SyncGuid,
    target_depth: &FetchDepth,
) -> Result<Option<(BookmarkTreeNode, Option<SyncGuid>, u32)>> {
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

    let scope = db.begin_interrupt_scope()?;

    let mut stmt = db.conn().prepare(sql)?;

    let mut results =
        stmt.query_and_then(&[(":item_guid", item_guid)], FetchedTreeRow::from_row)?;

    let parent_guid: Option<SyncGuid>;
    let position: u32;

    // The first row in the result set is always the root of our tree.
    let mut root = match results.next() {
        Some(result) => {
            let row = result?;
            parent_guid = row.parent_guid.clone();
            position = row.position;
            match row.node_type {
                BookmarkType::Folder => FolderNode {
                    guid: Some(row.guid.clone()),
                    date_added: Some(row.date_added),
                    last_modified: Some(row.last_modified),
                    title: row.title,
                    children: Vec::new(),
                }
                .into(),
                BookmarkType::Bookmark => {
                    // pretend invalid or missing URLs don't exist.
                    match row.url {
                        Some(str_val) => match Url::parse(str_val.as_str()) {
                            // an invalid URL presumably means a logic error
                            // somewhere far away from here...
                            Err(_) => return Ok(None),
                            Ok(url) => BookmarkNode {
                                guid: Some(row.guid.clone()),
                                date_added: Some(row.date_added),
                                last_modified: Some(row.last_modified),
                                title: row.title,
                                url,
                            }
                            .into(),
                        },
                        // This is double-extra-invalid because various
                        // constraints in the schema should prevent it (but we
                        // know from desktop's experience that on-disk
                        // corruption can cause it, so it's possible) - but
                        // we treat it as an `error` rather than just a `warn`
                        None => {
                            error_support::report_error!(
                                "places-bookmark-corruption",
                                "bookmark {:#} has missing url",
                                row.guid
                            );
                            return Ok(None);
                        }
                    }
                }
                BookmarkType::Separator => SeparatorNode {
                    guid: Some(row.guid.clone()),
                    date_added: Some(row.date_added),
                    last_modified: Some(row.last_modified),
                }
                .into(),
            }
        }
        None => return Ok(None),
    };

    // Skip the rest and return if root is not a folder
    if let BookmarkTreeNode::Bookmark { .. } | BookmarkTreeNode::Separator { .. } = root {
        return Ok(Some((root, parent_guid, position)));
    }

    scope.err_if_interrupted()?;
    // For all remaining rows, build a pseudo-tree that maps parent GUIDs to
    // ordered children. We need this intermediate step because SQLite returns
    // results in level order, so we'll see a node's siblings and cousins (same
    // level, but different parents) before any of their descendants.
    let mut pseudo_tree: HashMap<SyncGuid, Vec<BookmarkTreeNode>> = HashMap::new();
    for result in results {
        let row = result?;
        scope.err_if_interrupted()?;
        // Check if we have done fetching the asked depth
        if let FetchDepth::Specific(d) = *target_depth {
            if row.level as usize > d + 1 {
                break;
            }
        }
        let node = match row.node_type {
            BookmarkType::Bookmark => match &row.url {
                Some(url_str) => match Url::parse(url_str) {
                    Ok(url) => BookmarkNode {
                        guid: Some(row.guid.clone()),
                        date_added: Some(row.date_added),
                        last_modified: Some(row.last_modified),
                        title: row.title.clone(),
                        url,
                    }
                    .into(),
                    Err(e) => {
                        warn!(
                            "ignoring malformed bookmark {} - invalid URL: {:?}",
                            row.guid, e
                        );
                        continue;
                    }
                },
                None => {
                    warn!("ignoring malformed bookmark {} - no URL", row.guid);
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
    Ok(Some((root, parent_guid, position)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::test::new_mem_connection;
    use crate::storage::bookmarks::BookmarkRootGuid;
    use crate::tests::{assert_json_tree, assert_json_tree_with_depth};
    use serde_json::json;

    // These tests check the SQL that this JSON module does "behind the back" of the
    // main storage API.
    #[test]
    fn test_fetch_root() -> Result<()> {
        let conn = new_mem_connection();

        // Fetch the root
        let (t, _, _) =
            fetch_tree(&conn, &BookmarkRootGuid::Root.into(), &FetchDepth::Deepest)?.unwrap();
        let f = match t {
            BookmarkTreeNode::Folder { ref f } => f,
            _ => panic!("tree root must be a folder"),
        };
        assert_eq!(f.guid, Some(BookmarkRootGuid::Root.into()));
        assert_eq!(f.children.len(), 4);
        Ok(())
    }

    #[test]
    fn test_insert_tree_and_fetch_level() -> Result<()> {
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
        insert_tree(&conn, tree)?;

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
        // check it with deepest fetching level.
        assert_json_tree(&conn, &BookmarkRootGuid::Unfiled.into(), expected.clone());

        // check it with one level deep, which should be the same as the previous
        assert_json_tree_with_depth(
            &conn,
            &BookmarkRootGuid::Unfiled.into(),
            expected,
            &FetchDepth::Specific(1),
        );

        // check it with zero level deep, which should return root and its children only
        assert_json_tree_with_depth(
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
                        "children": [],
                    },
                    {
                        "title": "another bookmark",
                        "url": "https://www.example4.com/",
                    }
                ]
            }),
            &FetchDepth::Specific(0),
        );

        Ok(())
    }
}

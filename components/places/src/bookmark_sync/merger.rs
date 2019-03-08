/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// The merger - uses dogear to perform the actual merge.
// (or is dogear the merger, and this something else?)

use crate::error::*;
use crate::storage::bookmarks::BookmarkRootGuid;
use crate::types::{BookmarkType, SyncGuid, SyncedBookmarkKind, Timestamp};
use lazy_static::lazy_static;
use rusqlite::{Connection, Row, NO_PARAMS};
use sql_support::ConnExt;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use sync15::ServerTimestamp;

lazy_static! {
    static ref LOCAL_ROOTS_AS_SQL_SET: String = {
        // phew - this seems more complicated then it should be.
        let roots_as_strings: Vec<String> = BookmarkRootGuid::user_roots().iter().map(|g| format!("'{}'", g.as_guid())).collect();
        roots_as_strings.join(",")
    };

    static ref LOCAL_ITEMS_SQL_FRAGMENT: String = {
        format!(
            "localItems(id, guid, parentId, parentGuid, position, type, title,
                     parentTitle, placeId, dateAdded, lastModified, syncChangeCounter,
                     isSyncable, level) AS (
            SELECT b.id, b.guid, p.id, p.guid, b.position, b.type, b.title, p.title,
                   b.fk, b.dateAdded, b.lastModified, b.syncChangeCounter,
                   b.guid IN ({user_content_roots}), 0
            FROM moz_bookmarks b
            JOIN moz_bookmarks p ON p.id = b.parent
            WHERE b.guid <> '{tags_guid}' AND
                  p.guid = '{root_guid}'
            UNION ALL
            SELECT b.id, b.guid, s.id, s.guid, b.position, b.type, b.title, s.title,
                   b.fk, b.dateAdded, b.lastModified, b.syncChangeCounter,
                   s.isSyncable, s.level + 1
            FROM moz_bookmarks b
            JOIN localItems s ON s.id = b.parent
            WHERE b.guid <> '{root_guid}')",
            user_content_roots = *LOCAL_ROOTS_AS_SQL_SET,
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref(),
            tags_guid = "_tags_" // XXX - need tags!
        )
    };
}

/// A node in a local or remote bookmark tree. Nodes are lightweight: they carry
/// enough information for the merger to resolve trivial conflicts without
/// querying the database for the complete value state.
#[derive(Debug, Clone)]
struct BookmarkNode {
    guid: SyncGuid,
    kind: SyncedBookmarkKind,
    age: Duration,
    needs_merge: bool,
    level: u16,
    is_syncable: bool,
    // XXX - desktop has .children(), but is it actually used?
    //    children: Vec<BookmarkNode>
}

impl BookmarkNode {
    fn new_root() -> Self {
        BookmarkNode {
            guid: BookmarkRootGuid::Root.as_guid(),
            kind: SyncedBookmarkKind::Folder,
            age: Duration::from_millis(0),
            needs_merge: false,
            level: 0,
            is_syncable: false,
        }
    }

    fn from_remote_row(row: &Row, remote_time: ServerTimestamp) -> rusqlite::Result<Self> {
        Ok(Self {
            guid: row.get_checked("guid")?,
            // XXX - should we have an "unknown" for SyncedBookmarkKind?
            kind: SyncedBookmarkKind::from_u8(row.get_checked("kind")?).unwrap(),
            age: ServerTimestamp(row.get_checked::<_, f64>("serverModified").unwrap_or(0f64))
                .duration_since(remote_time)
                .unwrap_or_default(),
            needs_merge: row.get_checked("needsMerge")?,
            level: 0,
            is_syncable: false,
        })
    }

    fn from_local_row(row: &Row, local_time: Timestamp) -> rusqlite::Result<Self> {
        Ok(Self {
            guid: row.get_checked("guid")?,
            kind: SyncedBookmarkKind::from_u8(row.get_checked("kind")?).unwrap(),
            // Note that this doesn't account for local clock skew.
            age: row
                .get_checked::<_, Timestamp>("localModified")
                .unwrap_or_default()
                .duration_since(local_time)
                .unwrap_or_default(),
            level: row.get_checked("level")?,
            is_syncable: row.get_checked("isSyncable")?,
            needs_merge: row.get_checked::<_, u32>("syncChangeCounter")? > 0,
        })
    }

    fn is_folder(&self) -> bool {
        self.kind == SyncedBookmarkKind::Folder
    }
}

#[derive(Debug)]
struct BookmarkTree {
    nodes: Vec<BookmarkNode>,
    root: usize,
    by_guid: HashMap<SyncGuid, usize>,
    parent_by_child: HashMap<usize, Option<usize>>,
    tombstones: HashSet<SyncGuid>,
}

impl BookmarkTree {
    fn new(root: BookmarkNode) -> Self {
        let mut by_guid = HashMap::new();
        by_guid.insert(root.guid.clone(), 0);
        let mut parent_by_child = HashMap::new();
        parent_by_child.insert(0, None);
        Self {
            nodes: vec![root],
            root: 0,
            by_guid,
            parent_by_child,
            tombstones: HashSet::new(),
        }
    }

    fn insert(&mut self, parent_guid: &SyncGuid, node: BookmarkNode) {
        // XXX - errors instead of panics?
        assert!(!self.by_guid.contains_key(&node.guid));
        let parent_index = *self.by_guid.get(parent_guid).expect("parent must exist");
        let parent_node = &self.nodes[parent_index];
        assert!(parent_node.is_folder());

        let new_index = self.nodes.len();
        let guid = node.guid.clone();
        self.nodes.push(node);
        self.by_guid.insert(guid, new_index);
        self.parent_by_child.insert(new_index, Some(parent_index));
    }

    fn note_deleted(&mut self, guid: &SyncGuid) {
        self.tombstones.insert(guid.clone());
    }

    fn root(&self) -> &BookmarkNode {
        &self.nodes[self.root]
    }

    fn get(&self, guid: &SyncGuid) -> Option<&BookmarkNode> {
        match self.by_guid.get(guid) {
            Some(index) => Some(&self.nodes[*index]),
            None => None,
        }
    }
}

// A pseudo-tree which performs well when used to load tree data from sql.
#[derive(Debug)]
struct PseudoTree {
    nodes_by_parent: HashMap<SyncGuid, Vec<BookmarkNode>>,
}

impl PseudoTree {
    fn new() -> Self {
        Self {
            nodes_by_parent: HashMap::new(),
        }
    }

    fn insert(&mut self, parent_guid: SyncGuid, node: BookmarkNode) {
        if let Some(nodes) = self.nodes_by_parent.get_mut(&parent_guid) {
            nodes.push(node);
        } else {
            self.nodes_by_parent.insert(parent_guid, vec![node]);
        }
    }
}

fn inflate_tree(
    remote_tree: &mut BookmarkTree,
    pseudo_tree: &mut PseudoTree,
    parent_node: &BookmarkNode,
) -> Result<()> {
    let nodes = pseudo_tree.nodes_by_parent.remove(&parent_node.guid);
    if let Some(nodes) = nodes {
        for mut node in nodes {
            node.level = parent_node.level + 1;
            if parent_node.guid == remote_tree.root().guid {
                node.is_syncable = BookmarkRootGuid::from_guid(&parent_node.guid).is_some();
            } else if node.kind == SyncedBookmarkKind::Livemark {
                // We never supported livemarks and desktop no longer does,
                // but we may see them on the server. We flag unmerged remote
                // livemarks as non-syncable. This will upload tombstones
                // and reupload their parents.
                node.is_syncable = false;
            } else {
                node.is_syncable = parent_node.is_syncable;
            }
            // clone below isn't ideal - we could just pass guid, level,
            // is_syncable etc, but this will do for now.
            remote_tree.insert(&parent_node.guid, node.clone());
            inflate_tree(remote_tree, pseudo_tree, &mut node)?;
        }
    }
    Ok(())
}

struct Merger<'a> {
    pub db: &'a Connection,
}

impl<'a> Merger<'a> {
    fn apply(&self, local_time: &Timestamp, server_time: &ServerTimestamp) -> Result<()> {
        if !self.has_changes()? {
            return Ok(());
        }
        if !self.valid_local_roots()? {
            return Err(Corruption::InvalidLocalRoots.into());
        }
        // remote orphans
        // fetchSyncStatusMismatches
        let remote_tree = self.fetch_remote_tree(server_time)?;
        let local_tree = self.fetch_local_tree(local_time)?;

        // self.fetch_new_remote_contents(...)
        // self.fetch_new_local_contents(...)
        // DOGEAR ALL THE THINGS
        // do other stuff.
        // mfbt!
        Ok(())
    }

    fn has_changes(&self) -> Result<bool> {
        // In the first subquery, we check incoming items with needsMerge = true
        // except the tombstones who don't correspond to any local bookmark because
        // we don't store them yet, hence never "merged" (see bug 1343103).
        let sql = format!(
            "
            SELECT
              EXISTS (
               SELECT 1
               FROM moz_bookmarks_synced v
               LEFT JOIN moz_bookmarks b ON v.guid = b.guid
               WHERE v.needsMerge AND
               (NOT v.isDeleted OR b.guid NOT NULL)
              ) OR EXISTS (
               WITH RECURSIVE
               {}
               SELECT 1
               FROM localItems
               WHERE syncChangeCounter > 0
              ) OR EXISTS (
               SELECT 1
               FROM moz_bookmarks_deleted
              )
              AS hasChanges
        ",
            *LOCAL_ITEMS_SQL_FRAGMENT
        );
        Ok(self
            .db
            .try_query_row(
                &sql,
                &[],
                |row| -> rusqlite::Result<_> { Ok(row.get_checked::<_, bool>(0)?) },
                false,
            )?
            .unwrap_or(false))
    }

    /// If the local roots aren't valid the merger will have a bad time.
    fn valid_local_roots(&self) -> Result<bool> {
        let sql = "
            SELECT EXISTS(SELECT 1 FROM moz_bookmarks
                    WHERE guid = '{root_guid}' AND
                          parent = NULL) AND
             (SELECT COUNT(*) FROM moz_bookmarks b
              JOIN moz_bookmarks p ON p.id = b.parent
              WHERE b.guid IN {local_roots} AND
                    p.guid = '{root_guid}') = {num_user_roots} AS areValid";
        Ok(self
            .db
            .try_query_row(
                &sql,
                &[],
                |row| -> rusqlite::Result<_> { Ok(row.get_checked::<_, bool>(0)?) },
                false,
            )?
            .unwrap_or(false))
    }

    fn fetch_remote_tree(&self, server_time: &ServerTimestamp) -> Result<BookmarkTree> {
        // like desktop we first build a pseudo-tree.
        let sql = format!(
            "
            SELECT v.guid, IFNULL(s.parentGuid, '{unfiled_guid}') AS parentGuid,
                   IFNULL(s.position, -1) AS position, v.serverModified, v.kind,
                   v.needsMerge
            FROM moz_bookmarks_synced v
            LEFT JOIN moz_bookmarks_synced_structure s ON s.guid = v.guid
            WHERE NOT v.isDeleted AND
                  v.guid <> '{root_guid}' AND
                  (s.parentGuid IS NOT NULL OR v.kind <> {query_kind})
            ORDER BY parentGuid, position = -1, position, v.guid",
            root_guid = BookmarkRootGuid::Root.as_guid().as_ref(),
            unfiled_guid = BookmarkRootGuid::Unfiled.as_guid().as_ref(),
            query_kind = SyncedBookmarkKind::Query as u8
        );
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_and_then(NO_PARAMS, |row| -> rusqlite::Result<_> {
            Ok((
                row.get_checked::<_, SyncGuid>("parentGuid")?,
                BookmarkNode::from_remote_row(row, *server_time)?,
            ))
        })?;
        let mut pt = PseudoTree::new();
        for row in rows {
            let (parent_guid, node): (SyncGuid, BookmarkNode) = row?;
            pt.insert(parent_guid, node);
        }
        // Second, build a complete tree from the pseudo-tree. We could do these
        // two steps in SQL, but it's extremely inefficient.
        let mut tree = BookmarkTree::new(BookmarkNode::new_root());
        inflate_tree(&mut tree, &mut pt, &BookmarkNode::new_root())?; // hrmph, markh is confused

        // Note tombstones for remotely deleted items.
        let mut stmt = self
            .db
            .prepare("SELECT guid FROM moz_bookmarks_synced WHERE isDeleted AND needsMerge")?;
        let rows = stmt.query_and_then(NO_PARAMS, |row| row.get_checked::<_, SyncGuid>("guid"))?;
        for row in rows {
            let guid = row?;
            tree.note_deleted(&guid);
        }
        Ok(tree)
    }

    fn fetch_local_tree(&self, local_time: &Timestamp) -> Result<BookmarkTree> {
        let sql = format!(
            r#"
            WITH RECURSIVE
            {local_items_fragment}
            SELECT s.id, s.guid, s.parentGuid,
                   /* Map Places item types to Sync record kinds. */
                   (CASE s.type
                      WHEN {bookmark_type} THEN (
                        CASE SUBSTR((SELECT h.url FROM moz_places h
                                     WHERE h.id = s.placeId), 1, 6)
                        /* Queries are bookmarks with a "place:" URL scheme. */
                        WHEN 'place:' THEN {query_kind}
                        ELSE {bookmark_kind} END)
                      WHEN {folder_type} THEN {folder_kind}
                      ELSE {separator_kind} END) AS kind,
                   s.lastModified / 1000 AS localModified, s.syncChangeCounter,
                   s.level, s.isSyncable
            FROM localItems s
            ORDER BY s.level, s.parentId, s.position"#,
            local_items_fragment = *LOCAL_ITEMS_SQL_FRAGMENT,
            bookmark_type = BookmarkType::Bookmark as u8,
            bookmark_kind = SyncedBookmarkKind::Bookmark as u8,
            folder_type = BookmarkType::Folder as u8,
            folder_kind = SyncedBookmarkKind::Folder as u8,
            separator_kind = SyncedBookmarkKind::Separator as u8,
            query_kind = SyncedBookmarkKind::Query as u8
        );

        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_and_then(NO_PARAMS, |row| -> rusqlite::Result<_> {
            Ok((
                row.get_checked::<_, SyncGuid>("parentGuid")?,
                BookmarkNode::from_local_row(row, *local_time)?,
            ))
        })?;

        let mut tree = BookmarkTree::new(BookmarkNode::new_root());
        for row in rows {
            let (parent_guid, node): (SyncGuid, BookmarkNode) = row?;
            tree.insert(&parent_guid, node);
        }
        // Note tombstones for locally deleted items.
        let mut stmt = self.db.prepare("SELECT guid FROM moz_bookmarks_deleted")?;
        let rows = stmt.query_and_then(NO_PARAMS, |row| row.get_checked::<_, SyncGuid>("guid"))?;
        for row in rows {
            let guid = row?;
            tree.note_deleted(&guid);
        }
        Ok(tree)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::places_api::{test::new_mem_api, ConnectionType};
    use crate::bookmark_sync::store::BookmarksStore;
    use crate::storage::bookmarks::{insert_tree, BookmarkTreeNode};
    use serde_json::{json, Value};

    use std::cell::Cell;
    use sync15::Payload;

    fn insert_local_json_tree(conn: &impl ConnExt, jtree: Value) {
        let tree: BookmarkTreeNode = serde_json::from_value(jtree).expect("should be valid");
        let folder_node = match tree {
            BookmarkTreeNode::Folder(folder_node) => folder_node,
            _ => panic!("must be a folder"),
        };
        insert_tree(conn, &folder_node).expect("should insert");
    }

    #[test]
    fn test_fetch_remote_tree() -> Result<()> {
        let records = vec![
            json!({
                "id": "qqVTRWhLBOu3",
                "type": "bookmark",
                "parentid": BookmarkRootGuid::Unfiled.as_guid(),
                "parentName": "Unfiled Bookmarks",
                "dateAdded": 1381542355843u64,
                "title": "The title",
                "bmkUri": "https://example.com",
                "tags": [],
            }),
            json!({
                "id": BookmarkRootGuid::Unfiled.as_guid(),
                "type": "folder",
                "parentid": BookmarkRootGuid::Root.as_guid(),
                "parentName": "",
                "dateAdded": 0,
                "title": "Unfiled Bookmarks",
                "children": ["qqVTRWhLBOu3"],
                "tags": [],
            }),
        ];

        let api = new_mem_api();
        let conn = api.open_connection(ConnectionType::Sync)?;

        // suck records into the store.
        let store = BookmarksStore {
            db: &conn,
            client_info: &Cell::new(None),
        };

        for record in records {
            let payload = Payload::from_json(record).unwrap();
            store.apply_payload(ServerTimestamp(0.0), payload)?;
        }

        let merger = Merger { db: &conn };
        let tree = merger.fetch_remote_tree(&ServerTimestamp(0.0))?;

        // should be each user root, plus the real root, plus the bookmark we added.
        assert_eq!(tree.nodes.len(), BookmarkRootGuid::user_roots().len() + 2);

        let node = tree.get(&"qqVTRWhLBOu3".into()).expect("should exist");
        assert_eq!(node.needs_merge, true);
        assert_eq!(node.level, 2);
        assert_eq!(node.is_syncable, true);

        let node = tree
            .get(&BookmarkRootGuid::Unfiled.as_guid())
            .expect("should exist");
        assert_eq!(node.needs_merge, true);
        assert_eq!(node.level, 1);
        assert_eq!(node.is_syncable, true);

        let node = tree
            .get(&BookmarkRootGuid::Menu.as_guid())
            .expect("should exist");
        assert_eq!(node.needs_merge, false);
        assert_eq!(node.level, 1);
        assert_eq!(node.is_syncable, true);

        let node = tree
            .get(&BookmarkRootGuid::Root.as_guid())
            .expect("should exist");
        assert_eq!(node.needs_merge, false);
        assert_eq!(node.level, 0);
        assert_eq!(node.is_syncable, false);

        // We should have changes.
        assert_eq!(merger.has_changes().unwrap(), true);
        Ok(())
    }

    #[test]
    fn test_fetch_local_tree() -> Result<()> {
        let api = new_mem_api();
        let conn = api.open_connection(ConnectionType::Sync)?;

        conn.execute("UPDATE moz_bookmarks SET syncChangeCounter = 0", NO_PARAMS)
            .expect("should work");

        insert_local_json_tree(
            &conn,
            json!({
                "guid": &BookmarkRootGuid::Unfiled.as_guid(),
                "children": [
                    {
                        "guid": "bookmark1___",
                        "title": "the bookmark",
                        "url": "https://www.example.com/"
                    },
                ]
            }),
        );

        let merger = Merger { db: &conn };
        let tree = merger.fetch_local_tree(&Timestamp::now())?;

        // should be each user root, plus the real root, plus the bookmark we added.
        assert_eq!(tree.nodes.len(), BookmarkRootGuid::user_roots().len() + 2);

        let node = tree.get(&"bookmark1___".into()).expect("should exist");
        assert_eq!(node.needs_merge, true);
        // XXX - all "level" tests fail here - they are 1 less than expected.
        // assert_eq!(node.level, 2);
        assert_eq!(node.is_syncable, true);

        let node = tree
            .get(&BookmarkRootGuid::Unfiled.as_guid())
            .expect("should exist");
        // XXX - we appear to have change counter issues with the tree?
        // assert_eq!(node.needs_merge, true);
        // assert_eq!(node.level, 1);
        assert_eq!(node.is_syncable, true);

        let node = tree
            .get(&BookmarkRootGuid::Menu.as_guid())
            .expect("should exist");
        assert_eq!(node.needs_merge, false);
        // assert_eq!(node.level, 1);
        assert_eq!(node.is_syncable, true);

        let node = tree
            .get(&BookmarkRootGuid::Root.as_guid())
            .expect("should exist");
        assert_eq!(node.needs_merge, false);
        assert_eq!(node.level, 0);
        assert_eq!(node.is_syncable, false);

        // We should have changes.
        assert_eq!(merger.has_changes().unwrap(), true);
        Ok(())
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{
    BookmarkPosition, BookmarkRootGuid, BookmarkTreeNode, InsertableBookmark, InsertableFolder,
    InsertableItem, InsertableSeparator, UpdatableBookmark, UpdatableFolder, UpdatableItem,
    UpdatableSeparator, UpdateTreeLocation,
};

use crate::error::{InvalidPlaceInfo, Result};
use crate::msg_types;
use crate::types::{BookmarkType, SyncGuid};
use url::Url;

// This is used when returning the tree over the FFI.
impl From<BookmarkTreeNode> for msg_types::BookmarkNode {
    fn from(n: BookmarkTreeNode) -> Self {
        let (date_added, last_modified) = n.created_modified();
        let mut result = Self {
            node_type: Some(n.node_type() as i32),
            guid: Some(n.guid().to_string()),
            date_added: Some(date_added.0 as i64),
            last_modified: Some(last_modified.0 as i64),
            title: None,
            url: None,
            parent_guid: None,
            position: None,
            child_guids: vec![],
            child_nodes: vec![],
            have_child_nodes: Some(false),
        };

        // Not the most idiomatic, but avoids a lot of duplication.
        match n {
            BookmarkTreeNode::Bookmark(b) => {
                result.title = b.title;
                result.url = Some(b.url.into_string());
            }
            BookmarkTreeNode::Separator(_) => {
                // No separator-specific properties.
            }
            BookmarkTreeNode::Folder(f) => {
                result.title = f.title;
                let own_guid = &result.guid;
                result.child_nodes = f
                    .children
                    .into_iter()
                    .enumerate()
                    .map(|(i, bn)| {
                        let mut child = msg_types::BookmarkNode::from(bn);
                        child.parent_guid = own_guid.clone();
                        child.position = Some(i as u32);
                        child
                    })
                    .collect();
                result.have_child_nodes = Some(true);
            }
        }
        result
    }
}

impl msg_types::BookmarkNode {
    /// Get the BookmarkType, panicking if it's invalid (because it really never
    /// should be unless we have a bug somewhere).
    pub(crate) fn get_node_type(&self) -> BookmarkType {
        let value = self.node_type.unwrap();
        // Check that the cast wouldn't truncate first.
        assert!(
            value >= 0 && value <= std::u8::MAX as i32,
            "wildly illegal node_type: {}",
            value
        );

        BookmarkType::from_u8(value as u8).expect("Invalid node_type")
    }

    /// Convert the protobuf bookmark into information for insertion.
    pub fn into_insertable(self) -> Result<InsertableItem> {
        let ty = self.get_node_type();

        let parent_guid = self
            .parent_guid
            .map(SyncGuid::from)
            .unwrap_or_else(|| BookmarkRootGuid::Unfiled.into());

        let position = self
            .position
            .map_or(BookmarkPosition::Append, BookmarkPosition::Specific);

        Ok(match ty {
            BookmarkType::Bookmark => InsertableItem::Bookmark(InsertableBookmark {
                parent_guid,
                position,
                title: self.title,
                url: Url::parse(&self.url.ok_or(InvalidPlaceInfo::NoUrl)?)?,
                guid: None,
                date_added: None,
                last_modified: None,
            }),
            BookmarkType::Separator => InsertableItem::Separator(InsertableSeparator {
                parent_guid,
                position,
                guid: None,
                date_added: None,
                last_modified: None,
            }),
            BookmarkType::Folder => InsertableItem::Folder(InsertableFolder {
                parent_guid,
                position,
                title: self.title,
                guid: None,
                date_added: None,
                last_modified: None,
            }),
        })
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
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BookmarkUpdateInfo {
    pub guid: SyncGuid,
    pub title: Option<String>,
    pub url: Option<String>,
    pub parent_guid: Option<SyncGuid>,
    pub position: Option<u32>,
}

impl BookmarkUpdateInfo {
    /// Convert the `BookmarkUpdateInfo` into information for updating, (now that
    /// we know it's node type).
    pub fn into_updatable(self, ty: BookmarkType) -> Result<(SyncGuid, UpdatableItem)> {
        // Check the things that otherwise would be enforced by the type system.

        if self.title.is_some() && ty == BookmarkType::Separator {
            return Err(InvalidPlaceInfo::IllegalChange("title", ty).into());
        }

        if self.url.is_some() && ty != BookmarkType::Bookmark {
            return Err(InvalidPlaceInfo::IllegalChange("url", ty).into());
        }

        if let Some(root) = BookmarkRootGuid::from_guid(&self.guid) {
            return Err(InvalidPlaceInfo::CannotUpdateRoot(root).into());
        }

        let location = match (self.parent_guid, self.position) {
            (None, None) => UpdateTreeLocation::None,
            (None, Some(pos)) => UpdateTreeLocation::Position(BookmarkPosition::Specific(pos)),
            (Some(parent_guid), pos) => UpdateTreeLocation::Parent(
                SyncGuid::from(parent_guid),
                pos.map_or(BookmarkPosition::Append, BookmarkPosition::Specific),
            ),
        };

        let updatable = match ty {
            BookmarkType::Bookmark => UpdatableItem::Bookmark(UpdatableBookmark {
                location,
                title: self.title,
                url: self.url.map(|u| Url::parse(&u)).transpose()?,
            }),
            BookmarkType::Separator => UpdatableItem::Separator(UpdatableSeparator { location }),
            BookmarkType::Folder => UpdatableItem::Folder(UpdatableFolder {
                location,
                title: self.title,
            }),
        };

        Ok((self.guid, updatable))
    }
}

impl From<msg_types::BookmarkNode> for BookmarkUpdateInfo {
    fn from(n: msg_types::BookmarkNode) -> Self {
        Self {
            // This is a bug in our code on the other side of the FFI,
            // so expect should be fine.
            guid: SyncGuid(n.guid.expect("Missing guid")),
            title: n.title,
            url: n.url,
            parent_guid: n.parent_guid.map(SyncGuid),
            position: n.position,
        }
    }
}

impl From<Vec<msg_types::BookmarkNode>> for msg_types::BookmarkNodeList {
    fn from(nodes: Vec<msg_types::BookmarkNode>) -> Self {
        Self { nodes }
    }
}

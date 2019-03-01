/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{
    BookmarkPosition, BookmarkRootGuid, BookmarkTreeNode, InsertableBookmark, InsertableFolder,
    InsertableItem, InsertableSeparator, UpdatableBookmark, UpdatableFolder, UpdatableItem,
    UpdatableSeparator, UpdateTreeLocation,
};

use crate::error::{Error, ErrorKind, Result};
use crate::msg_types;
use crate::types::{BookmarkType, SyncGuid};
use url::Url;

// This is used when returning the tree over the FFI.
impl From<BookmarkTreeNode> for msg_types::BookmarkNode {
    fn from(n: BookmarkTreeNode) -> Self {
        let (date_added, last_modified) = n.created_modified();
        let mut result = Self {
            node_type: n.node_type() as u8 as i32,
            guid: Some(n.guid().to_string()),
            date_added: Some(date_added.0 as i64),
            last_modified: Some(last_modified.0 as i64),
            title: None,
            url: None,
            parent_guid: None,
            position: None,
            child_guids: vec![],
            child_nodes: vec![],
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
                result.child_guids = f.children.iter().map(|g| g.guid().to_string()).collect();
                result.child_nodes = f
                    .children
                    .into_iter()
                    .map(msg_types::BookmarkNode::from)
                    .collect();
            }
        }
        result
    }
}

fn bad_insertion(reason: impl Into<String>) -> Error {
    Error::from(ErrorKind::BadBookmarkInsertion(reason.into()))
}

fn bad_update(reason: impl Into<String>) -> Error {
    Error::from(ErrorKind::BadBookmarkUpdate(reason.into()))
}

macro_rules! insertion_check {
    ($cnd:expr, $($fmt_args:tt)*) => {
        if !$cnd {
            return Err(bad_insertion(format!($($fmt_args)*)));
        }
    };
}

macro_rules! update_check {
    ($cnd:expr, $($fmt_args:tt)*) => {
        if !$cnd {
            return Err(bad_update(format!($($fmt_args)*)));
        }
    };
}

impl msg_types::BookmarkNode {
    /// Get the BookmarkType, panicking if it's invalid (because it really never should be unless
    /// we have a bug somewhere).
    pub(crate) fn node_type(&self) -> BookmarkType {
        // Check that the cast wouldn't truncate first.
        assert!(
            self.node_type >= 0 && self.node_type <= std::u8::MAX as i32,
            "wildly illegal node_type: {}",
            self.node_type
        );

        BookmarkType::from_u8(self.node_type as u8).expect("Invalid node_type")
    }

    /// Convert the protobuf bookmark into information for insertion.
    pub fn into_insertable(self) -> Result<InsertableItem> {
        let ty = self.node_type();

        insertion_check!(
            self.guid.is_none(),
            "Guid may not be provided when creating a bookmark"
        );

        insertion_check!(
            self.last_modified.is_none() && self.date_added.is_none(),
            "Neither date_added nor last_modified may be provided when creating a bookmark"
        );

        insertion_check!(
            self.child_nodes.is_empty() && self.child_guids.is_empty(),
            "Children may not be provided when creating a folder"
        );

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
                url: Url::parse(
                    &self
                        .url
                        .ok_or_else(|| bad_insertion("No URL provided for bookmark insertion"))?,
                )?,
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

    /// Convert the protobuf bookmark into information for updating.
    pub fn into_updatable(self) -> Result<(SyncGuid, UpdatableItem)> {
        let ty = self.node_type();
        let guid = self
            .guid
            .ok_or_else(|| bad_update("Guid must be provided when updating a bookmark"))?;

        update_check!(
            self.last_modified.is_none() && self.date_added.is_none(),
            "Neither date_added nor last_modified may be provided when updating a bookmark"
        );

        update_check!(
            self.child_nodes.is_empty() && self.child_guids.is_empty(),
            "Children may not be provided when updating a folder"
        );

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

        Ok((SyncGuid::from(guid), updatable))
    }
}

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{
    BookmarkPosition, BookmarkRootGuid, BookmarkTreeNode, BookmarkUpdateInfo, InsertableBookmark,
    InsertableFolder, InsertableItem, InsertableSeparator, InvalidPlaceInfo, PublicNode,
    RawBookmark, UpdatableBookmark, UpdatableFolder, UpdatableItem, UpdatableSeparator,
    UpdateTreeLocation,
};

use crate::error::Result;
use crate::msg_types;
use crate::types::BookmarkType;
use sync_guid::Guid as SyncGuid;
use url::Url;

impl From<BookmarkTreeNode> for PublicNode {
    // TODO: Eventually this should either be a function that takes an
    // SqlInterruptScope, or we should have another version that does.
    // For now it is likely fine.
    fn from(n: BookmarkTreeNode) -> Self {
        let (date_added, last_modified) = n.created_modified();
        let mut result = Self {
            node_type: n.node_type(),
            guid: n.guid().clone(),
            date_added,
            last_modified,
            ..Default::default()
        };

        // Not the most idiomatic, but avoids a lot of duplication.
        match n {
            BookmarkTreeNode::Bookmark { b } => {
                result.title = b.title;
                result.url = Some(b.url);
            }
            BookmarkTreeNode::Separator { .. } => {
                // No separator-specific properties.
            }
            BookmarkTreeNode::Folder { f } => {
                result.title = f.title;
                let own_guid = &result.guid;
                result.child_nodes = Some(
                    f.children
                        .into_iter()
                        .enumerate()
                        .map(|(i, bn)| {
                            let mut child = PublicNode::from(bn);
                            child.parent_guid = Some(own_guid.clone());
                            child.position = i as u32;
                            child
                        })
                        .collect(),
                );
            }
        }
        result
    }
}

impl From<PublicNode> for msg_types::BookmarkNode {
    fn from(n: PublicNode) -> Self {
        let have_child_nodes = if n.node_type == BookmarkType::Folder {
            Some(n.child_nodes.is_some())
        } else {
            None
        };
        Self {
            node_type: Some(n.node_type as i32),
            guid: Some(n.guid.into_string()),
            date_added: Some(n.date_added.0 as i64),
            last_modified: Some(n.last_modified.0 as i64),
            title: n.title,
            url: n.url.map(String::from),
            parent_guid: n.parent_guid.map(|g| g.into_string()),
            position: Some(n.position),
            child_guids: n.child_guids.map_or(vec![], |child_guids| {
                child_guids
                    .into_iter()
                    .map(|m| m.into_string())
                    .collect::<Vec<String>>()
            }),
            child_nodes: n.child_nodes.map_or(vec![], |nodes| {
                nodes
                    .into_iter()
                    .map(msg_types::BookmarkNode::from)
                    .collect()
            }),
            have_child_nodes,
        }
    }
}

// Note: this conversion is incomplete if rb is a folder!
impl From<RawBookmark> for PublicNode {
    fn from(rb: RawBookmark) -> Self {
        Self {
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
    }
}

impl From<Vec<PublicNode>> for msg_types::BookmarkNodeList {
    fn from(ns: Vec<PublicNode>) -> Self {
        Self {
            nodes: ns.into_iter().map(msg_types::BookmarkNode::from).collect(),
        }
    }
}

impl msg_types::BookmarkNode {
    /// Get the BookmarkType, panicking if it's invalid (because it really never
    /// should be unless we have a bug somewhere).
    pub(crate) fn get_node_type(&self) -> BookmarkType {
        let value = self.node_type.unwrap();
        // Check that the cast wouldn't truncate first.
        assert!(
            value >= 0 && value <= i32::from(std::u8::MAX),
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

        let position =
            self.position
                .map_or(BookmarkPosition::Append, |pos| BookmarkPosition::Specific {
                    pos,
                });

        Ok(match ty {
            BookmarkType::Bookmark => InsertableItem::Bookmark {
                b: InsertableBookmark {
                    parent_guid,
                    position,
                    title: self.title,
                    // This will fail if Url is empty, but with a url parse error,
                    // which is what we want.
                    url: Url::parse(&self.url.unwrap_or_default())?,
                    guid: None,
                    date_added: None,
                    last_modified: None,
                },
            },
            BookmarkType::Separator => InsertableItem::Separator {
                s: InsertableSeparator {
                    parent_guid,
                    position,
                    guid: None,
                    date_added: None,
                    last_modified: None,
                },
            },
            BookmarkType::Folder => InsertableItem::Folder {
                f: InsertableFolder {
                    parent_guid,
                    position,
                    title: self.title,
                    guid: None,
                    date_added: None,
                    last_modified: None,
                },
            },
        })
    }
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

        let location = match (self.parent_guid, self.position) {
            (None, None) => UpdateTreeLocation::None,
            (None, Some(pos)) => UpdateTreeLocation::Position {
                pos: BookmarkPosition::Specific { pos },
            },
            (Some(parent_guid), pos) => UpdateTreeLocation::Parent {
                guid: parent_guid,
                pos: pos.map_or(BookmarkPosition::Append, |p| BookmarkPosition::Specific {
                    pos: p,
                }),
            },
        };

        let updatable = match ty {
            BookmarkType::Bookmark => UpdatableItem::Bookmark {
                b: UpdatableBookmark {
                    location,
                    title: self.title,
                    url: self.url.map(|u| Url::parse(&u)).transpose()?,
                },
            },
            BookmarkType::Separator => UpdatableItem::Separator {
                s: UpdatableSeparator { location },
            },
            BookmarkType::Folder => UpdatableItem::Folder {
                f: UpdatableFolder {
                    location,
                    title: self.title,
                },
            },
        };

        Ok((self.guid, updatable))
    }
}

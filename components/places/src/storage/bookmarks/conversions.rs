/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use super::{
    BookmarkPosition, BookmarkTreeNode, BookmarkUpdateInfo, InvalidPlaceInfo, PublicNode,
    RawBookmark, UpdatableBookmark, UpdatableFolder, UpdatableItem, UpdatableSeparator,
    UpdateTreeLocation,
};

use crate::error::Result;
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
